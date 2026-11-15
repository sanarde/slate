// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

const RETRY: Duration = Duration::from_secs(2);

pub type Roles = HashMap<String, u32>;

pub struct Scheme {
    pub roles: Roles,
    pub dark: bool,
}

pub struct Seurat {
    stream: Option<UnixStream>,
    pending: Vec<u8>,
    block: Option<Scheme>,
    scheme: Option<Scheme>,
    last_attempt: Option<Instant>,
}

const LIGHT: [(&str, u32); 38] = [
    ("primary", 0x6750a4),
    ("on_primary", 0xffffff),
    ("primary_container", 0xeaddff),
    ("on_primary_container", 0x21005d),
    ("secondary", 0x625b71),
    ("on_secondary", 0xffffff),
    ("secondary_container", 0xe8def8),
    ("on_secondary_container", 0x1d192b),
    ("tertiary", 0x7d5260),
    ("on_tertiary", 0xffffff),
    ("tertiary_container", 0xffd8e4),
    ("on_tertiary_container", 0x31111d),
    ("error", 0xb3261e),
    ("on_error", 0xffffff),
    ("error_container", 0xf9dedc),
    ("on_error_container", 0x410e0b),
    ("background", 0xfef7ff),
    ("on_background", 0x1d1b20),
    ("surface", 0xfef7ff),
    ("on_surface", 0x1d1b20),
    ("surface_variant", 0xe7e0ec),
    ("on_surface_variant", 0x49454f),
    ("outline", 0x79747e),
    ("outline_variant", 0xcac4d0),
    ("shadow", 0x000000),
    ("scrim", 0x000000),
    ("inverse_surface", 0x322f35),
    ("inverse_on_surface", 0xf5eff7),
    ("inverse_primary", 0xd0bcff),
    ("surface_dim", 0xded8e1),
    ("surface_bright", 0xfef7ff),
    ("surface_container_lowest", 0xffffff),
    ("surface_container_low", 0xf7f2fa),
    ("surface_container", 0xf3edf7),
    ("surface_container_high", 0xece6f0),
    ("surface_container_highest", 0xe6e0e9),
    ("primary_fixed", 0xeaddff),
    ("primary_fixed_dim", 0xd0bcff),
];

const DARK: [(&str, u32); 38] = [
    ("primary", 0xd0bcff),
    ("on_primary", 0x381e72),
    ("primary_container", 0x4f378b),
    ("on_primary_container", 0xeaddff),
    ("secondary", 0xccc2dc),
    ("on_secondary", 0x332d41),
    ("secondary_container", 0x4a4458),
    ("on_secondary_container", 0xe8def8),
    ("tertiary", 0xefb8c8),
    ("on_tertiary", 0x492532),
    ("tertiary_container", 0x633b48),
    ("on_tertiary_container", 0xffd8e4),
    ("error", 0xf2b8b5),
    ("on_error", 0x601410),
    ("error_container", 0x8c1d18),
    ("on_error_container", 0xf9dedc),
    ("background", 0x141218),
    ("on_background", 0xe6e0e9),
    ("surface", 0x141218),
    ("on_surface", 0xe6e0e9),
    ("surface_variant", 0x49454f),
    ("on_surface_variant", 0xcac4d0),
    ("outline", 0x938f99),
    ("outline_variant", 0x49454f),
    ("shadow", 0x000000),
    ("scrim", 0x000000),
    ("inverse_surface", 0xe6e0e9),
    ("inverse_on_surface", 0x322f35),
    ("inverse_primary", 0x6750a4),
    ("surface_dim", 0x141218),
    ("surface_bright", 0x3b383e),
    ("surface_container_lowest", 0x0f0d13),
    ("surface_container_low", 0x1d1b20),
    ("surface_container", 0x211f26),
    ("surface_container_high", 0x2b2930),
    ("surface_container_highest", 0x36343b),
    ("primary_fixed", 0xeaddff),
    ("primary_fixed_dim", 0xd0bcff),
];

pub fn baseline(dark: bool) -> Roles {
    let table = if dark { &DARK } else { &LIGHT };
    table.iter().map(|(name, value)| (name.to_string(), *value)).collect()
}

fn parse_color(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

impl Seurat {
    pub fn new() -> Seurat {
        Seurat {
            stream: None,
            pending: Vec::new(),
            block: None,
            scheme: None,
            last_attempt: None,
        }
    }

    pub fn scheme(&self) -> Option<&Scheme> {
        self.scheme.as_ref()
    }

    fn connect(&mut self) {
        let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
            return;
        };
        let path = std::path::Path::new(&runtime).join("seurat.sock");
        let Ok(stream) = UnixStream::connect(path) else {
            return;
        };
        if stream.set_nonblocking(true).is_ok() {
            tracing::info!("connected to seurat");
            self.stream = Some(stream);
        }
    }

    pub fn poll(&mut self) -> bool {
        if self.stream.is_none() {
            if self.last_attempt.is_some_and(|attempt| attempt.elapsed() < RETRY) {
                return false;
            }
            self.last_attempt = Some(Instant::now());
            self.connect();
        }
        let Some(stream) = self.stream.as_mut() else {
            return false;
        };

        let mut buffer = [0u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    self.stream = None;
                    break;
                }
                Ok(count) => self.pending.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.stream = None;
                    break;
                }
            }
        }
        self.parse()
    }

    fn parse(&mut self) -> bool {
        let mut changed = false;
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim();

            if line.starts_with("scheme ") {
                self.block = Some(Scheme {
                    roles: Roles::new(),
                    dark: line.contains("mode=dark"),
                });
            } else if line == "end" {
                if let Some(block) = self.block.take() {
                    self.scheme = Some(block);
                    changed = true;
                }
            } else if let Some(block) = self.block.as_mut()
                && let Some((role, value)) = line.split_once('=')
                && let Some(color) = parse_color(value)
            {
                block.roles.insert(role.trim().to_string(), color);
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scheme_block_replaces_the_previous_one() {
        let mut seurat = Seurat::new();
        seurat.pending.extend_from_slice(
            b"scheme source=#6750a4 mode=dark\nprimary=#d0bcff\nbogus=zzz\nend\nscheme source=#112233 mode=light\nprimary=#112233\n",
        );

        assert!(seurat.parse());
        let scheme = seurat.scheme().unwrap();
        assert!(scheme.dark);
        assert_eq!(scheme.roles.get("primary"), Some(&0xd0bcff));
        assert!(!scheme.roles.contains_key("bogus"));

        seurat.pending.extend_from_slice(b"end\n");
        assert!(seurat.parse());
        let scheme = seurat.scheme().unwrap();
        assert!(!scheme.dark);
        assert_eq!(scheme.roles.get("primary"), Some(&0x112233));
    }

    #[test]
    fn the_baseline_has_the_roles_themes_use() {
        for dark in [false, true] {
            let roles = baseline(dark);
            for name in ["primary", "on_surface", "surface_container_high", "outline_variant", "error"] {
                assert!(roles.contains_key(name), "{name}");
            }
        }
    }
}

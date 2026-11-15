// SPDX-License-Identifier: GPL-3.0-only

use std::process::Command;

use fontdue::{Font, FontSettings, Metrics};

const ELLIPSIS: char = '\u{2026}';

pub struct Fonts {
    faces: Vec<Font>,
}

pub struct Glyph {
    pub character: char,
    face: usize,
    pub x: f32,
}

pub struct Layout {
    pub glyphs: Vec<Glyph>,
    pub width: f32,
}

fn resolve(pattern: &str) -> Option<String> {
    let output = Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

fn load_face(pattern: &str) -> Option<Font> {
    let path = if pattern.starts_with('/') {
        pattern.to_string()
    } else {
        match resolve(pattern) {
            Some(path) => path,
            None => {
                tracing::warn!(pattern, "fc-match found no font");
                return None;
            }
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(%error, path, "failed to read a font");
            return None;
        }
    };
    match Font::from_bytes(bytes, FontSettings::default()) {
        Ok(font) => Some(font),
        Err(error) => {
            tracing::warn!(error, path, "failed to parse a font");
            None
        }
    }
}

impl Fonts {
    pub fn load(patterns: &str) -> Fonts {
        let faces = patterns
            .split(',')
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .filter_map(load_face)
            .collect();
        Fonts { faces }
    }

    fn face_for(&self, character: char) -> Option<usize> {
        self.faces
            .iter()
            .position(|face| face.lookup_glyph_index(character) != 0)
            .or(if self.faces.is_empty() { None } else { Some(0) })
    }

    fn advance(&self, character: char, size: f32) -> f32 {
        self.face_for(character)
            .map(|face| self.faces[face].metrics(character, size).advance_width)
            .unwrap_or(0.0)
    }

    pub fn ascent_and_descent(&self, size: f32) -> Option<(f32, f32)> {
        let metrics = self.faces.first()?.horizontal_line_metrics(size)?;
        Some((metrics.ascent, metrics.descent))
    }

    pub fn layout(&self, text: &str, size: f32, max_width: f32) -> Layout {
        let place = |characters: &[char]| -> Layout {
            let mut glyphs = Vec::with_capacity(characters.len());
            let mut pen = 0.0;
            for &character in characters {
                let Some(face) = self.face_for(character) else {
                    continue;
                };
                glyphs.push(Glyph {
                    character,
                    face,
                    x: pen,
                });
                pen += self.advance(character, size);
            }
            Layout { glyphs, width: pen }
        };

        let characters: Vec<char> = text.chars().filter(|character| !character.is_control()).collect();
        let full = place(&characters);
        if full.width <= max_width {
            return full;
        }

        let mut keep = characters.len();
        while keep > 0 {
            keep -= 1;
            let mut shortened: Vec<char> = characters[..keep].to_vec();
            shortened.push(ELLIPSIS);
            let layout = place(&shortened);
            if layout.width <= max_width || keep == 0 {
                return layout;
            }
        }
        Layout {
            glyphs: Vec::new(),
            width: 0.0,
        }
    }

    pub fn rasterize(&self, glyph: &Glyph, size: f32) -> (Metrics, Vec<u8>) {
        self.faces[glyph.face].rasterize(glyph.character, size)
    }
}

// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::delegate_dmabuf;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};

use super::super::state::Slate;

impl DmabufHandler for Slate {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let imported = self.udev.as_mut().is_some_and(|udev| udev.import_dmabuf(&dmabuf));
        if imported {
            if notifier.successful::<Slate>().is_err() {
                tracing::warn!("client of an imported dmabuf is gone");
            }
        } else {
            notifier.failed();
        }
    }
}

delegate_dmabuf!(Slate);

//! One owner thread renders each editor. Network viewers never own EditorApp or the PTY.
use super::{
    protocol::{Control, Frame, Input, Texture},
    renderer::Renderer,
};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

struct Viewer {
    remote: bool,
    revision: Option<u64>,
    epoch: u64,
    last_seen: Instant,
    commands: Vec<eframe::egui::OutputCommand>,
    paste: bool,
}

pub(super) struct Runtime {
    pub renderer: Renderer,
    viewers: HashMap<String, Viewer>,
    owner: Option<String>,
    epoch: u64,
    revision: u64,
    latest: Option<Frame>,
    history: VecDeque<(u64, Vec<Texture>, Vec<String>)>,
    history_bytes: usize,
    last_render: Instant,
}

impl Runtime {
    pub fn new(arguments: &crate::logging::Arguments) -> Self {
        Self {
            renderer: Renderer::new(arguments),
            viewers: HashMap::new(),
            owner: None,
            epoch: 1,
            revision: 0,
            latest: None,
            history: VecDeque::new(),
            history_bytes: 0,
            last_render: Instant::now(),
        }
    }
    pub fn connect(&mut self, client: &str, remote: bool) -> Control {
        self.viewers.insert(
            client.into(),
            Viewer {
                remote,
                revision: None,
                epoch: 0,
                last_seen: Instant::now(),
                commands: vec![],
                paste: false,
            },
        );
        if self.owner.is_none() && !remote {
            self.claim(client);
        }
        self.control(client)
    }
    pub fn control(&self, client: &str) -> Control {
        Control {
            owned: self.owner.as_deref() == Some(client),
            owner: self.owner.as_ref().map(|id| {
                if self.viewers.get(id).is_some_and(|v| v.remote) {
                    "Remote browser"
                } else {
                    "Local window"
                }
                .into()
            }),
            epoch: self.epoch,
        }
    }
    pub fn known(&self, client: &str) -> bool {
        self.viewers.contains_key(client)
    }
    pub fn remote(&self, client: &str) -> bool {
        self.viewers.get(client).is_some_and(|viewer| viewer.remote)
    }
    pub fn claim(&mut self, client: &str) -> Control {
        if self.known(client) && self.owner.as_deref() != Some(client) {
            self.renderer.release_input();
            self.owner = Some(client.into());
            self.epoch += 1;
            for viewer in self.viewers.values_mut() {
                viewer.commands.clear();
                viewer.paste = false;
            }
        }
        self.control(client)
    }
    pub fn disconnect(&mut self, client: &str) {
        self.viewers.remove(client);
        if self.owner.as_deref() == Some(client) {
            self.renderer.release_input();
            self.owner = None;
            self.epoch += 1;
        }
    }
    pub fn tick(&mut self) {
        let expired: Vec<_> = self
            .viewers
            .iter()
            .filter(|(_, view)| view.last_seen.elapsed() > Duration::from_secs(30))
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.disconnect(&id);
        }
        if self.last_render.elapsed() >= Duration::from_millis(100) || self.latest.is_none() {
            let remote = self.owner.as_ref().is_some_and(|owner| self.remote(owner));
            if let Ok(frame) = self.renderer.render_frame(None, remote) {
                self.publish(frame);
            }
        }
    }
    fn publish(&mut self, mut frame: Frame) {
        self.last_render = Instant::now();
        if let Some(viewer) = self
            .owner
            .as_ref()
            .and_then(|owner| self.viewers.get_mut(owner))
        {
            viewer.commands.append(&mut frame.output.commands);
            viewer.paste |= frame.paste;
        }
        frame.output.commands.clear();
        frame.paste = false;
        frame.output.events.clear();
        frame.output.num_completed_passes = 0;
        let changed = self.latest.as_ref().is_none_or(|previous| {
            previous.meshes != frame.meshes
                || previous.size != frame.size
                || previous.title != frame.title
                || previous.dirty != frame.dirty
                || previous.pixels_per_point != frame.pixels_per_point
                || previous.output != frame.output
        }) || !frame.textures.is_empty()
            || !frame.free.is_empty();
        if changed {
            self.revision += 1;
            let textures = std::mem::take(&mut frame.textures);
            self.history_bytes += textures.iter().map(|t| t.pixels.len()).sum::<usize>();
            self.history
                .push_back((self.revision, textures, std::mem::take(&mut frame.free)));
            while self.history.len() > 64 || self.history_bytes > 32 * 1024 * 1024 {
                if let Some((_, textures, _)) = self.history.pop_front() {
                    self.history_bytes -= textures.iter().map(|t| t.pixels.len()).sum::<usize>();
                } else {
                    break;
                }
            }
            self.latest = Some(frame);
        }
    }
    pub fn frame(&mut self, client: &str, input: Input) -> Result<Option<Frame>, String> {
        if !self.known(client) {
            return Err("Connect to this editor first".into());
        }
        self.viewers.get_mut(client).unwrap().last_seen = Instant::now();
        if self.control(client).owned && input.epoch == Some(self.epoch) {
            // Process logical key/pointer boundaries locally, avoiding one WAN round trip per key.
            let mut remaining = input.events.clone().into_iter().peekable();
            let mut frames = Vec::new();
            let mut batch = Vec::new();
            let mut key = false;
            let mut text = false;
            while let Some(event) = remaining.next() {
                let pressed = matches!(event, eframe::egui::Event::Key { pressed: true, .. });
                if pressed && (key || text) && !batch.is_empty() {
                    frames.push(std::mem::take(&mut batch));
                    key = false;
                    text = false;
                }
                key |= pressed;
                text |= matches!(
                    event,
                    eframe::egui::Event::Text(_)
                        | eframe::egui::Event::Paste(_)
                        | eframe::egui::Event::Ime(_)
                );
                let boundary = matches!(
                    event,
                    eframe::egui::Event::PointerButton { .. }
                        | eframe::egui::Event::Copy
                        | eframe::egui::Event::Cut
                        | eframe::egui::Event::Key {
                            key: eframe::egui::Key::Enter
                                | eframe::egui::Key::Tab
                                | eframe::egui::Key::Backspace
                                | eframe::egui::Key::Delete
                                | eframe::egui::Key::ArrowUp
                                | eframe::egui::Key::ArrowDown
                                | eframe::egui::Key::ArrowLeft
                                | eframe::egui::Key::ArrowRight,
                            pressed: true,
                            ..
                        }
                ) || matches!(&event, eframe::egui::Event::Key { pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command);
                batch.push(event);
                if boundary {
                    frames.push(std::mem::take(&mut batch));
                    key = false;
                    text = false;
                }
            }
            if !batch.is_empty() || frames.is_empty() {
                frames.push(batch);
            }
            if frames.len() > 512 {
                return Err("Too many input actions in one request".into());
            }
            for events in frames {
                let mut next = input.clone();
                next.events = events;
                let frame = self
                    .renderer
                    .render_frame(Some(next), self.remote(client))?;
                self.publish(frame);
            }
        } else {
            self.tick();
        }
        self.snapshot(client)
    }
    fn snapshot(&mut self, client: &str) -> Result<Option<Frame>, String> {
        if self.latest.is_none() {
            self.tick();
        }
        let control = self.control(client);
        let viewer = self.viewers.get_mut(client).ok_or("Unknown viewer")?;
        if viewer.revision == Some(self.revision)
            && viewer.epoch == control.epoch
            && viewer.commands.is_empty()
            && !viewer.paste
        {
            return Ok(None);
        }
        let mut frame = self
            .latest
            .as_ref()
            .ok_or("Editor frame is not ready")?
            .clone();
        let reset = viewer.revision.is_none()
            || self
                .history
                .front()
                .is_some_and(|(revision, _, _)| viewer.revision.unwrap_or(0) + 1 < *revision);
        if reset {
            frame.textures = self.renderer.atlas();
            frame.free.clear();
        } else {
            for (_, textures, free) in self
                .history
                .iter()
                .filter(|(revision, _, _)| *revision > viewer.revision.unwrap_or(0))
            {
                frame.textures.extend(textures.iter().cloned());
                frame.free.extend(free.iter().cloned());
            }
        }
        frame.control = control;
        frame.reset = reset;
        frame.revision = self.revision;
        frame.output.commands = std::mem::take(&mut viewer.commands);
        frame.paste = std::mem::take(&mut viewer.paste);
        viewer.revision = Some(self.revision);
        viewer.epoch = self.epoch;
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::Arguments;
    use eframe::egui;

    #[test]
    fn shared_frames_control_epochs_transients_and_expiry() {
        if std::env::var_os("FOCUS_RUNTIME_TEST").is_none() {
            let root = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "browser::runtime::tests::shared_frames_control_epochs_transients_and_expiry",
                    "--nocapture",
                ])
                .env("FOCUS_RUNTIME_TEST", "1")
                .env("SHELL", "/bin/sh")
                .env("XDG_DATA_HOME", root.path().join("data"))
                .env("XDG_CONFIG_HOME", root.path().join("config"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let mut runtime = Runtime::new(&Arguments::default());
        let local = runtime.connect("local", false);
        assert!(local.owned);
        assert!(!runtime.connect("remote", true).owned);
        let input = |epoch, size| Input {
            epoch: Some(epoch),
            size,
            pixels_per_point: 1.0,
            focused: true,
            events: vec![],
        };
        let local_frame = runtime
            .frame("local", input(local.epoch, [1200.0, 800.0]))
            .unwrap()
            .unwrap();
        let observer = runtime
            .frame("remote", input(local.epoch, [600.0, 400.0]))
            .unwrap()
            .unwrap();
        assert_eq!(observer.size, local_frame.size);
        assert!(observer.reset && !observer.textures.is_empty());
        let remote = runtime.claim("remote");
        assert!(remote.owned && remote.epoch > local.epoch);
        runtime
            .frame("remote", input(remote.epoch, [1000.0, 700.0]))
            .unwrap();
        let stale = runtime
            .frame("local", input(local.epoch, [2000.0, 1500.0]))
            .unwrap()
            .unwrap();
        assert_eq!(stale.size, [1000.0, 700.0]);
        assert!(!stale.control.owned);
        let mut frame = runtime.renderer.render_frame(None, true).unwrap();
        frame
            .output
            .commands
            .push(egui::OutputCommand::CopyText("controller-only".into()));
        frame.paste = true;
        runtime.publish(frame);
        if let Some(observer) = runtime.snapshot("local").unwrap() {
            assert!(observer.output.commands.is_empty() && !observer.paste);
        }
        let delivered = runtime.snapshot("remote").unwrap().unwrap();
        assert_eq!(delivered.output.commands.len(), 1);
        assert!(delivered.paste);
        assert!(runtime.snapshot("remote").unwrap().is_none());
        // Lagging viewers receive a complete texture atlas after delta retention expires.
        for number in 0..70 {
            let mut frame = runtime.latest.clone().unwrap();
            frame.title = Some(format!("Revision {number}"));
            runtime.publish(frame);
        }
        let replay = runtime.snapshot("local").unwrap().unwrap();
        assert!(replay.reset && replay.textures.iter().all(|texture| texture.pos.is_none()));
        let mut frame = runtime.latest.clone().unwrap();
        frame
            .output
            .commands
            .push(egui::OutputCommand::CopyText("old-owner".into()));
        runtime.publish(frame);
        let reclaimed = runtime.claim("local");
        assert!(reclaimed.owned);
        assert!(
            runtime
                .snapshot("remote")
                .unwrap()
                .unwrap()
                .output
                .commands
                .is_empty()
        );
        runtime.viewers.get_mut("local").unwrap().last_seen =
            Instant::now() - Duration::from_secs(31);
        runtime.tick();
        assert!(!runtime.known("local"));
        assert!(runtime.control("remote").owner.is_none());
        let previous = runtime.control("remote").epoch;
        runtime.claim("unknown");
        assert_eq!(runtime.control("remote").epoch, previous);
    }
}

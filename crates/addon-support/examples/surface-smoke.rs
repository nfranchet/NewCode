//! Exercise a built surface addon with autostart disabled, without starting its backend.
use focus_addon_sdk::{Context, Message, read_message, write_message};
use focus_addon_support::surface::{Frame, Input};
use std::{
    io::BufReader,
    process::{Command, Stdio},
};
fn main() {
    for executable in std::env::args().skip(1) {
        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut context = Context {
            generation: 1,
            workspace: Some(std::env::current_dir().unwrap()),
            storage: std::env::temp_dir().join(format!("focus-addon-smoke-{}", std::process::id())),
            state: serde_json::json!({"autostart":false}),
            ..Default::default()
        };
        write_message(
            &mut input,
            &Message::request(
                0,
                "initialize",
                serde_json::json!({"protocol":1,"context":context}),
            ),
        )
        .unwrap();
        let reply = read_message(&mut output).unwrap().unwrap();
        assert_eq!(reply.result.unwrap()["surface"], 1);
        for generation in [1, 2] {
            if generation == 2 {
                context.generation = 2;
                write_message(
                    &mut input,
                    &Message::notification("context.changed", &context),
                )
                .unwrap();
            }
            let request = Input {
                generation,
                sequence: generation,
                pixels_per_point: 1.25,
                input: egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(600.0, 30.0),
                        egui::vec2(400.0, 650.0),
                    )),
                    ..Default::default()
                },
            };
            write_message(&mut input, &Message::notification("surface.input", request)).unwrap();
            loop {
                let message = read_message(&mut output)
                    .unwrap()
                    .expect("Addon exited before returning a frame");
                if message.method.as_deref() != Some("surface.frame") {
                    continue;
                }
                let frame: Frame = serde_json::from_value(message.params.unwrap()).unwrap();
                assert_eq!(frame.generation, generation);
                assert!(!frame.meshes.is_empty());
                for texture in &frame.textures {
                    texture.decode().unwrap();
                }
                break;
            }
        }
        write_message(&mut input, &Message::notification("shutdown", ())).unwrap();
        assert!(child.wait().unwrap().success());
        println!("Surface IPC and workspace switch passed: {executable}");
    }
}

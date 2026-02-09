use std::{env, thread, time::Duration};

use tau_tui::{
    apply_overlay, Component, DiffRenderer, EditorBuffer, EditorView, LumaImage, Text, Theme,
    ThemeRole,
};

const HELP: &str = "\
tau-tui demo runner

Usage:
  cargo run -p tau-tui -- [--frames N] [--width N] [--sleep-ms N] [--no-color]

Options:
  --frames N    Number of demo frames to render (default: 3, min: 1)
  --width N     Render width in characters (default: 72, min: 20)
  --sleep-ms N  Delay between frames in milliseconds (default: 120)
  --no-color    Disable ANSI color output for CI/smoke runs
  --help, -h    Show this help message
";

#[derive(Debug, Clone, PartialEq, Eq)]
struct DemoArgs {
    frames: usize,
    width: usize,
    sleep_ms: u64,
    color: bool,
}

impl Default for DemoArgs {
    fn default() -> Self {
        Self {
            frames: 3,
            width: 72,
            sleep_ms: 120,
            color: true,
        }
    }
}

#[derive(Debug)]
enum ParseAction {
    Run(DemoArgs),
    Help,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<ParseAction, String> {
    let mut parsed = DemoArgs::default();
    let mut it = args.into_iter();
    let _ = it.next();

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(ParseAction::Help),
            "--no-color" => parsed.color = false,
            "--frames" => {
                let raw = it.next().ok_or("missing value for --frames")?;
                let value = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid usize for --frames: {raw}"))?;
                if value == 0 {
                    return Err("--frames must be >= 1".to_string());
                }
                parsed.frames = value;
            }
            "--width" => {
                let raw = it.next().ok_or("missing value for --width")?;
                let value = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid usize for --width: {raw}"))?;
                if value < 20 {
                    return Err("--width must be >= 20".to_string());
                }
                parsed.width = value;
            }
            "--sleep-ms" => {
                let raw = it.next().ok_or("missing value for --sleep-ms")?;
                let value = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid u64 for --sleep-ms: {raw}"))?;
                parsed.sleep_ms = value;
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok(ParseAction::Run(parsed))
}

fn paint(theme: &Theme, role: ThemeRole, text: impl Into<String>, color: bool) -> String {
    let text = text.into();
    if color {
        theme.paint(role, &text)
    } else {
        text
    }
}

fn compose_frame(
    buffer: &EditorBuffer,
    image: &LumaImage,
    args: &DemoArgs,
    frame: usize,
) -> Vec<String> {
    let viewport_top = buffer.lines().len().saturating_sub(6);
    let editor_lines = EditorView::new(buffer)
        .with_viewport(viewport_top, 6)
        .with_line_numbers(true)
        .with_cursor(true)
        .render(args.width);

    let mut base = Text::new("live editor view").render(args.width);
    base.extend(editor_lines);
    base.push(String::new());
    base.push("ascii preview".to_string());
    base.extend(image.render_fit(args.width.min(24)));

    let overlay = vec![format!(
        "frame={}/{} width={} sleep_ms={}",
        frame + 1,
        args.frames,
        args.width,
        args.sleep_ms
    )];
    apply_overlay(&base, &overlay, 0, 0)
}

fn advance_buffer(buffer: &mut EditorBuffer, frame: usize) {
    if frame == 0 {
        buffer.insert_text("fn tau_demo_loop(frame: usize) {\n    let status = \"ready\";\n");
        buffer.insert_text("    println!(\"frame={frame} status={status}\");\n}");
        return;
    }
    buffer.insert_newline();
    buffer.insert_text(&format!(
        "// frame {} checkpoint: render diff + overlay",
        frame + 1
    ));
}

fn run_demo(args: DemoArgs) -> Result<(), String> {
    let theme = Theme::default();
    let image = LumaImage::from_luma(
        8,
        4,
        vec![
            0, 24, 56, 88, 120, 152, 184, 216, 16, 40, 72, 104, 136, 168, 200, 232, 32, 64, 96,
            128, 160, 192, 224, 255, 24, 56, 88, 120, 152, 184, 216, 248,
        ],
    )
    .map_err(|err| format!("failed to construct demo image: {err}"))?;
    let mut buffer = EditorBuffer::new();
    let mut diff = DiffRenderer::new();

    for frame in 0..args.frames {
        advance_buffer(&mut buffer, frame);
        let rendered = compose_frame(&buffer, &image, &args, frame);
        let operations = diff.diff(rendered.clone());

        let header = paint(
            &theme,
            ThemeRole::Accent,
            format!(
                "Tau TUI Demo - frame {}/{} (ops={})",
                frame + 1,
                args.frames,
                operations.len()
            ),
            args.color,
        );
        println!("{header}");
        for operation in operations {
            let line = paint(
                &theme,
                ThemeRole::Muted,
                format!("op:{operation}"),
                args.color,
            );
            println!("{line}");
        }
        for line in rendered {
            println!("{}", paint(&theme, ThemeRole::Primary, line, args.color));
        }
        println!();

        if frame + 1 < args.frames && args.sleep_ms > 0 {
            thread::sleep(Duration::from_millis(args.sleep_ms));
        }
    }
    Ok(())
}

fn main() {
    let action = match parse_args(env::args()) {
        Ok(action) => action,
        Err(err) => {
            eprintln!("{err}");
            eprintln!();
            eprintln!("{HELP}");
            std::process::exit(2);
        }
    };

    match action {
        ParseAction::Help => {
            println!("{HELP}");
        }
        ParseAction::Run(args) => {
            if let Err(err) = run_demo(args) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{compose_frame, parse_args, ParseAction};
    use tau_tui::{EditorBuffer, LumaImage};

    #[test]
    fn unit_parse_args_defaults_are_stable() {
        let action = parse_args(vec!["tau-tui".to_string()]).expect("parse succeeds");
        let ParseAction::Run(parsed) = action else {
            panic!("expected run action");
        };
        assert_eq!(parsed.frames, 3);
        assert_eq!(parsed.width, 72);
        assert_eq!(parsed.sleep_ms, 120);
        assert!(parsed.color);
    }

    #[test]
    fn functional_parse_args_supports_custom_values() {
        let action = parse_args(vec![
            "tau-tui".to_string(),
            "--frames".to_string(),
            "5".to_string(),
            "--width".to_string(),
            "90".to_string(),
            "--sleep-ms".to_string(),
            "0".to_string(),
            "--no-color".to_string(),
        ])
        .expect("parse succeeds");
        let ParseAction::Run(parsed) = action else {
            panic!("expected run action");
        };
        assert_eq!(parsed.frames, 5);
        assert_eq!(parsed.width, 90);
        assert_eq!(parsed.sleep_ms, 0);
        assert!(!parsed.color);
    }

    #[test]
    fn regression_parse_args_rejects_zero_frames() {
        let err = parse_args(vec![
            "tau-tui".to_string(),
            "--frames".to_string(),
            "0".to_string(),
        ])
        .expect_err("expected parse failure");
        assert!(err.contains("--frames must be >= 1"));
    }

    #[test]
    fn regression_compose_frame_overlays_frame_metadata() {
        let mut buffer = EditorBuffer::new();
        buffer.insert_text("let tau = true;");
        let image = LumaImage::from_luma(2, 2, vec![0, 128, 200, 255]).expect("image");
        let args = super::DemoArgs {
            frames: 2,
            width: 40,
            sleep_ms: 0,
            color: false,
        };

        let frame = compose_frame(&buffer, &image, &args, 0);
        assert!(!frame.is_empty());
        assert!(frame[0].contains("frame=1/2"));
        assert!(frame.iter().any(|line| line.contains("ascii preview")));
    }
}

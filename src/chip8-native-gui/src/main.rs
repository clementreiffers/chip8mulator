mod app;
mod audio;
mod debug;
mod input;
mod renderer;

use std::{error::Error, ffi::OsString, path::PathBuf, sync::Arc, time::Instant};

use app::App;
use chip8_engine::CompatibilityProfile;
use input::key_to_chip8;
use renderer::Renderer;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::WindowBuilder,
};

const DEFAULT_PALETTE: [egui::Color32; 16] = [
    egui::Color32::from_rgb(2, 4, 12),
    egui::Color32::from_rgb(51, 255, 186),
    egui::Color32::from_rgb(255, 64, 190),
    egui::Color32::WHITE,
    egui::Color32::from_rgb(49, 128, 255),
    egui::Color32::from_rgb(255, 190, 66),
    egui::Color32::from_rgb(183, 102, 255),
    egui::Color32::from_rgb(90, 235, 255),
    egui::Color32::from_rgb(255, 94, 94),
    egui::Color32::from_rgb(110, 255, 112),
    egui::Color32::from_rgb(255, 142, 58),
    egui::Color32::from_rgb(255, 115, 225),
    egui::Color32::from_rgb(108, 170, 255),
    egui::Color32::from_rgb(255, 230, 92),
    egui::Color32::from_rgb(154, 255, 225),
    egui::Color32::from_rgb(230, 236, 255),
];

fn main() -> Result<(), Box<dyn Error>> {
    let (rom_path, debug_mode, profile, palette) = parse_args(std::env::args_os().skip(1))?;
    let rom = std::fs::read(&rom_path)?;
    let title = format!("CHIP-8 — {}", rom_path.display());
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(title)
            .with_inner_size(if debug_mode {
                LogicalSize::new(1_280.0, 800.0)
            } else {
                LogicalSize::new(960.0, 480.0)
            })
            .with_min_inner_size(LogicalSize::new(320.0, 160.0))
            .build(&event_loop)?,
    );
    let mut renderer = pollster::block_on(Renderer::new(Arc::clone(&window)))?;
    let egui_ctx = egui::Context::default();
    let mut egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        window.as_ref(),
        Some(window.scale_factor() as f32),
        None,
    );
    let mut frame_texture = None;
    let mut app = App::new(rom, debug_mode, profile)?;
    let _audio = match audio::AudioOutput::open(app.audio_state()) {
        Ok(output) => Some(output),
        Err(error) => {
            eprintln!("audio disabled: {error}");
            None
        }
    };
    let mut last_frame = Instant::now();

    event_loop.run(move |event, event_loop| {
        event_loop.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { window_id, event } if window_id == window.id() => {
                let egui_response = egui_state.on_window_event(window.as_ref(), &event);
                match event {
                    WindowEvent::CloseRequested => event_loop.exit(),
                    WindowEvent::Resized(size) => renderer.resize(size),
                    WindowEvent::KeyboardInput { event, .. } if !egui_response.consumed => {
                        let pressed = event.state == ElementState::Pressed;
                        if let PhysicalKey::Code(code) = event.physical_key {
                            if pressed && code == winit::keyboard::KeyCode::Escape {
                                event_loop.exit();
                                return;
                            }
                            if pressed && app.handle_command(code) {
                                if app.is_halted() {
                                    event_loop.exit();
                                }
                                return;
                            }
                            if let Some(key) = key_to_chip8(code)
                                && let Err(error) = app.set_key(key, pressed)
                            {
                                eprintln!("input error: {error}");
                                event_loop.exit();
                            }
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        match app.advance(now.duration_since(last_frame)) {
                            Ok(true) => {
                                event_loop.exit();
                                return;
                            }
                            Ok(false) => {}
                            Err(error) => {
                                eprintln!("emulation error: {error}");
                                event_loop.exit();
                                return;
                            }
                        }
                        last_frame = now;
                        let upload_frame = app.take_frame_dirty();
                        let raw_input = egui_state.take_egui_input(window.as_ref());
                        let mut debug_activated = false;
                        let output = egui_ctx.run(raw_input, |ctx| {
                            debug_activated = show_interface(
                                ctx,
                                &mut app,
                                &mut frame_texture,
                                upload_frame,
                                &palette,
                            );
                        });
                        if debug_activated {
                            let _ = window.request_inner_size(LogicalSize::new(1_280.0, 800.0));
                        }
                        egui_state.handle_platform_output(
                            window.as_ref(),
                            output.platform_output.clone(),
                        );
                        let pixels_per_point = egui_ctx.pixels_per_point();
                        let paint_jobs =
                            egui_ctx.tessellate(output.shapes.clone(), pixels_per_point);
                        match renderer.render(output, &paint_jobs, pixels_per_point) {
                            Ok(()) => app.mark_frame_presented(),
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                renderer.reconfigure()
                            }
                            Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                            Err(wgpu::SurfaceError::Timeout) => {}
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    })?;
    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = OsString>,
) -> Result<(PathBuf, bool, CompatibilityProfile, [egui::Color32; 16]), String> {
    let mut rom_path = None;
    let mut debug_mode = false;
    let mut profile = CompatibilityProfile::OriginalChip8;
    let mut palette = DEFAULT_PALETTE;
    while let Some(arg) = args.next() {
        if arg == "--debug-mode" {
            debug_mode = true;
        } else if arg == "--profile" {
            let value = args.next().ok_or_else(|| {
                format!(
                    "--profile requires a value (chip8, chip48, modern, superchip, xochip)\n\n{}",
                    usage()
                )
            })?;
            profile = match value.to_string_lossy().as_ref() {
                "chip8" => CompatibilityProfile::OriginalChip8,
                "chip48" => CompatibilityProfile::Chip48,
                "modern" => CompatibilityProfile::Modern,
                "superchip" => CompatibilityProfile::SuperChip,
                "xochip" => CompatibilityProfile::XoChip,
                _ => {
                    return Err(format!(
                        "unknown profile: {}\n\n{}",
                        value.to_string_lossy(),
                        usage()
                    ));
                }
            };
        } else if arg == "--palette" {
            let value = args.next().ok_or_else(|| {
                format!(
                    "--palette requires four #RRGGBB colors separated by commas\n\n{}",
                    usage()
                )
            })?;
            palette = parse_palette(&value)?;
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!(
                "unknown option: {}\n\n{}",
                arg.to_string_lossy(),
                usage()
            ));
        } else if rom_path.replace(PathBuf::from(arg)).is_some() {
            return Err(format!("only one ROM can be provided\n\n{}", usage()));
        }
    }
    rom_path
        .map(|path| (path, debug_mode, profile, palette))
        .ok_or_else(|| usage().to_owned())
}

fn parse_palette(value: &OsString) -> Result<[egui::Color32; 16], String> {
    let value = value.to_string_lossy();
    let colors: Vec<_> = value
        .split(',')
        .map(parse_color)
        .collect::<Result<_, _>>()?;
    let colors: [egui::Color32; 4] = colors.try_into().map_err(|_| {
        format!(
            "--palette requires exactly four #RRGGBB colors, got {value}\n\n{}",
            usage()
        )
    })?;
    let mut palette = DEFAULT_PALETTE;
    palette[..4].copy_from_slice(&colors);
    Ok(palette)
}

fn parse_color(value: &str) -> Result<egui::Color32, String> {
    let Some(hex) = value.strip_prefix('#') else {
        return Err(format!(
            "invalid palette color {value}; expected #RRGGBB\n\n{}",
            usage()
        ));
    };
    if hex.len() != 6 {
        return Err(format!(
            "invalid palette color {value}; expected #RRGGBB\n\n{}",
            usage()
        ));
    }
    let component = |range| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| {
            format!(
                "invalid palette color {value}; expected #RRGGBB\n\n{}",
                usage()
            )
        })
    };
    Ok(egui::Color32::from_rgb(
        component(0..2)?,
        component(2..4)?,
        component(4..6)?,
    ))
}

fn usage() -> &'static str {
    "usage: chip8-native-gui [--debug-mode] [--profile chip8|chip48|modern|superchip|xochip] [--palette #RRGGBB,#RRGGBB,#RRGGBB,#RRGGBB] <rom.ch8>\n\nControls: Space pause, F10 step (debug), F5 restart, F1/F2/F3/F4/F6 compatibility profile, Esc quit."
}

fn show_interface(
    ctx: &egui::Context,
    app: &mut App,
    frame_texture: &mut Option<egui::TextureHandle>,
    upload_frame: bool,
    palette: &[egui::Color32; 16],
) -> bool {
    let image = frame_image(app.framebuffer(), app.display_dimensions(), palette);
    if let Some(texture) = frame_texture
        && texture.size() == image.size
    {
        if upload_frame {
            texture.set(image, egui::TextureOptions::NEAREST);
        }
    } else {
        *frame_texture =
            Some(ctx.load_texture("chip8-frame", image, egui::TextureOptions::NEAREST));
    }
    if app.is_debug_enabled() {
        show_debug_interface(
            ctx,
            app,
            frame_texture.as_ref().expect("frame texture initialized"),
        );
        false
    } else {
        let mut debug_activated = false;
        egui::TopBottomPanel::top("window_options").show(ctx, |ui| {
            ui.menu_button("Options", |ui| {
                if ui.button("Activer le mode debug").clicked() {
                    app.enable_debug();
                    debug_activated = true;
                    ui.close_menu();
                }
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(0.0))
            .show(ctx, |ui| {
                show_screen(
                    ui,
                    frame_texture.as_ref().expect("frame texture initialized"),
                    None,
                )
            });
        debug_activated
    }
}

fn frame_image(
    framebuffer: &[u8],
    dimensions: (usize, usize),
    palette: &[egui::Color32; 16],
) -> egui::ColorImage {
    let mut image = egui::ColorImage::new([dimensions.0, dimensions.1], palette[0]);
    for (output, input) in image.pixels.iter_mut().zip(framebuffer) {
        *output = palette
            .get(usize::from(*input))
            .copied()
            .unwrap_or(palette[0]);
    }
    image
}

fn show_debug_interface(ctx: &egui::Context, app: &mut App, texture: &egui::TextureHandle) {
    let mut toggle_pause = false;
    let mut step = false;
    let mut restart = false;
    let mut profile = None;
    egui::TopBottomPanel::top("debug_controls").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui
                .button(if app.is_paused() {
                    "Reprendre"
                } else {
                    "Pause"
                })
                .clicked()
            {
                toggle_pause = true;
            }
            if ui.button("Pas-à-pas (F10)").clicked() {
                step = true;
            }
            if ui.button("Redémarrer (F5)").clicked() {
                restart = true;
            }
            ui.separator();
            if ui.button("CHIP-8 (F1)").clicked() {
                profile = Some(CompatibilityProfile::OriginalChip8);
            }
            if ui.button("CHIP-48 (F2)").clicked() {
                profile = Some(CompatibilityProfile::Chip48);
            }
            if ui.button("Modern (F3)").clicked() {
                profile = Some(CompatibilityProfile::Modern);
            }
            if ui.button("Super-CHIP (F4)").clicked() {
                profile = Some(CompatibilityProfile::SuperChip);
            }
            if ui.button("XO-CHIP (F6)").clicked() {
                profile = Some(CompatibilityProfile::XoChip);
            }
        });
    });
    egui::SidePanel::right("debug_metrics")
        .min_width(260.0)
        .show(ctx, |ui| {
            ui.heading("Performance");
            ui.label(if app.is_paused() {
                "État : en pause"
            } else {
                "État : exécution"
            });
            let entries: Vec<_> = app.debug().expect("debug enabled").trace().iter().collect();
            let last = entries.last().map(|entry| entry.analysis_time);
            let average = (!entries.is_empty()).then(|| {
                entries
                    .iter()
                    .map(|entry| entry.analysis_time.as_secs_f64())
                    .sum::<f64>()
                    / entries.len() as f64
            });
            let response = entries.last().and_then(|entry| entry.response_time);
            ui.label(format_duration("Analyse", last));
            ui.label(format_duration(
                "Analyse moyenne",
                average.map(std::time::Duration::from_secs_f64),
            ));
            ui.label(format_duration("Réponse", response));
            ui.separator();
            ui.label("Durée d'analyse (historique)");
            draw_chart(ui, &entries);
            if ui.button("Effacer les breakpoints").clicked() {
                app.debug_mut().expect("debug enabled").clear_breakpoints();
            }
        });
    egui::CentralPanel::default().show(ctx, |ui| {
        show_screen(ui, texture, Some(640.0));
        ui.separator();
        ui.heading("Instructions exécutées");
        let mut toggled_breakpoint = None;
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                egui::Grid::new("trace_grid")
                    .striped(true)
                    .min_col_width(72.0)
                    .show(ui, |ui| {
                        ui.label("BP");
                        ui.label("PC");
                        ui.label("Opcode");
                        ui.label("Instruction");
                        ui.label("Analyse");
                        ui.label("Réponse");
                        ui.end_row();
                        let debug = app.debug_mut().expect("debug enabled");
                        for entry in debug.trace().iter() {
                            let mut breakpoint = debug.is_breakpoint(entry.pc);
                            if ui.checkbox(&mut breakpoint, "").changed() {
                                toggled_breakpoint = Some(entry.pc);
                            }
                            ui.monospace(format!("{:#05X}", entry.pc));
                            ui.monospace(format!("{:#06X}", entry.opcode));
                            ui.monospace(&entry.mnemonic);
                            ui.monospace(format!(
                                "{:.2} µs",
                                entry.analysis_time.as_secs_f64() * 1_000_000.0
                            ));
                            ui.monospace(entry.response_time.map_or_else(
                                || "—".into(),
                                |time| format!("{:.2} ms", time.as_secs_f64() * 1_000.0),
                            ));
                            ui.end_row();
                        }
                        if let Some(pc) = toggled_breakpoint {
                            debug.toggle_breakpoint(pc);
                        }
                    });
            });
    });
    if toggle_pause {
        app.toggle_pause();
    }
    if step {
        let _ = app.step_once();
    }
    if restart {
        let _ = app.handle_command(winit::keyboard::KeyCode::F5);
    }
    if let Some(profile) = profile {
        let key = match profile {
            CompatibilityProfile::OriginalChip8 => winit::keyboard::KeyCode::F1,
            CompatibilityProfile::Chip48 => winit::keyboard::KeyCode::F2,
            CompatibilityProfile::Modern => winit::keyboard::KeyCode::F3,
            CompatibilityProfile::SuperChip => winit::keyboard::KeyCode::F4,
            CompatibilityProfile::XoChip => winit::keyboard::KeyCode::F6,
        };
        let _ = app.handle_command(key);
    }
}

fn show_screen(ui: &mut egui::Ui, texture: &egui::TextureHandle, maximum_width: Option<f32>) {
    let width = maximum_width.map_or_else(
        || ui.available_width(),
        |maximum| ui.available_width().min(maximum),
    );
    let [texture_width, texture_height] = texture.size();
    ui.image((
        texture.id(),
        egui::vec2(width, width * texture_height as f32 / texture_width as f32),
    ));
}

fn draw_chart(ui: &mut egui::Ui, entries: &[&debug::TraceEntry]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 120.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::DARK_GRAY));
    let maximum = entries
        .iter()
        .map(|entry| entry.analysis_time.as_secs_f32())
        .fold(0.0_f32, f32::max)
        .max(0.000_001);
    for pair in entries.windows(2).enumerate() {
        let (index, values) = pair;
        let x = |position: usize| {
            rect.left() + rect.width() * position as f32 / (entries.len() - 1).max(1) as f32
        };
        let y = |value: f32| rect.bottom() - rect.height() * (value / maximum);
        painter.line_segment(
            [
                egui::pos2(x(index), y(values[0].analysis_time.as_secs_f32())),
                egui::pos2(x(index + 1), y(values[1].analysis_time.as_secs_f32())),
            ],
            egui::Stroke::new(1.5, egui::Color32::LIGHT_GREEN),
        );
    }
}

fn format_duration(label: &str, duration: Option<std::time::Duration>) -> String {
    duration.map_or_else(
        || format!("{label} : —"),
        |value| format!("{label} : {:.2} µs", value.as_secs_f64() * 1_000_000.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_debug_flag_in_any_position() {
        let args = [OsString::from("--debug-mode"), OsString::from("rom.ch8")];
        assert_eq!(
            parse_args(args.into_iter()).expect("valid args"),
            (
                PathBuf::from("rom.ch8"),
                true,
                CompatibilityProfile::OriginalChip8,
                DEFAULT_PALETTE
            )
        );
        let args = [OsString::from("rom.ch8"), OsString::from("--debug-mode")];
        assert_eq!(
            parse_args(args.into_iter()).expect("valid args"),
            (
                PathBuf::from("rom.ch8"),
                true,
                CompatibilityProfile::OriginalChip8,
                DEFAULT_PALETTE
            )
        );
    }

    #[test]
    fn parses_superchip_profile() {
        let args = [
            OsString::from("--profile"),
            OsString::from("superchip"),
            OsString::from("rom.ch8"),
        ];
        assert_eq!(
            parse_args(args.into_iter()).expect("valid args"),
            (
                PathBuf::from("rom.ch8"),
                false,
                CompatibilityProfile::SuperChip,
                DEFAULT_PALETTE
            )
        );
    }

    #[test]
    fn parses_xochip_profile() {
        let args = [
            OsString::from("--profile"),
            OsString::from("xochip"),
            OsString::from("rom.ch8"),
        ];
        assert_eq!(
            parse_args(args.into_iter()).expect("valid args").2,
            CompatibilityProfile::XoChip
        );
    }

    #[test]
    fn parses_four_custom_palette_colours_and_preserves_extended_colours() {
        let args = [
            OsString::from("--palette"),
            OsString::from("#87CEEB,#554422,#456543,#EEEEFF"),
            OsString::from("rom.ch8"),
        ];
        let palette = parse_args(args.into_iter()).expect("valid palette").3;
        assert_eq!(palette[0], egui::Color32::from_rgb(135, 206, 235));
        assert_eq!(palette[1], egui::Color32::from_rgb(85, 68, 34));
        assert_eq!(palette[2], egui::Color32::from_rgb(69, 101, 67));
        assert_eq!(palette[3], egui::Color32::from_rgb(238, 238, 255));
        assert_eq!(palette[4], DEFAULT_PALETTE[4]);
    }

    #[test]
    fn rejects_invalid_palette_arguments() {
        let missing_colour = [
            OsString::from("--palette"),
            OsString::from("#000000,#111111,#222222"),
            OsString::from("rom.ch8"),
        ];
        assert!(parse_args(missing_colour.into_iter()).is_err());

        let invalid_hex = [
            OsString::from("--palette"),
            OsString::from("#000000,#111111,#22222G,#333333"),
            OsString::from("rom.ch8"),
        ];
        assert!(parse_args(invalid_hex.into_iter()).is_err());
    }

    #[test]
    fn framebuffer_pixels_use_visible_debug_colors() {
        let mut framebuffer = vec![0; 64 * 32];
        framebuffer[1] = 1;
        framebuffer[2] = 2;
        framebuffer[3] = 3;
        let image = frame_image(&framebuffer, (64, 32), &DEFAULT_PALETTE);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(2, 4, 12));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(51, 255, 186));
        assert_eq!(image.pixels[2], egui::Color32::from_rgb(255, 64, 190));
        assert_eq!(image.pixels[3], egui::Color32::WHITE);
    }

    #[test]
    fn framebuffer_uses_custom_standard_palette_and_keeps_extended_default_colours() {
        let palette = parse_palette(&OsString::from("#010203,#040506,#070809,#0A0B0C"))
            .expect("valid palette");
        let image = frame_image(&[0, 1, 2, 3, 4], (5, 1), &palette);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(1, 2, 3));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(4, 5, 6));
        assert_eq!(image.pixels[2], egui::Color32::from_rgb(7, 8, 9));
        assert_eq!(image.pixels[3], egui::Color32::from_rgb(10, 11, 12));
        assert_eq!(image.pixels[4], DEFAULT_PALETTE[4]);
    }
}

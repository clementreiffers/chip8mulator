mod app;
mod input;
mod renderer;

use std::{error::Error, path::PathBuf, sync::Arc, time::Instant};

use app::App;
use input::key_to_chip8;
use renderer::Renderer;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::WindowBuilder,
};

fn main() -> Result<(), Box<dyn Error>> {
    let rom_path = std::env::args_os().nth(1).map(PathBuf::from).ok_or(
        "usage: chip8-native-gui <rom.ch8>\n\nControls: Space pause, F5 restart, F1/F2/F3 compatibility profile, Esc quit.",
    )?;
    let rom = std::fs::read(&rom_path)?;
    let title = format!("CHIP-8 — {}", rom_path.display());

    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(title)
            .with_inner_size(LogicalSize::new(960.0, 480.0))
            .with_min_inner_size(LogicalSize::new(320.0, 160.0))
            .build(&event_loop)?,
    );
    let mut renderer = pollster::block_on(Renderer::new(Arc::clone(&window)))?;
    let mut app = App::new(rom)?;
    let mut last_frame = Instant::now();

    event_loop.run(move |event, event_loop| {
        event_loop.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, event } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => renderer.resize(size),
                WindowEvent::KeyboardInput { event, .. } => {
                    let pressed = event.state == ElementState::Pressed;
                    if let PhysicalKey::Code(code) = event.physical_key {
                        if pressed && code == winit::keyboard::KeyCode::Escape {
                            event_loop.exit();
                            return;
                        }
                        if pressed && app.handle_command(code) {
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
                    if let Err(error) = app.advance(now.duration_since(last_frame)) {
                        eprintln!("emulation error: {error}");
                        event_loop.exit();
                        return;
                    }
                    last_frame = now;
                    let upload_frame = app.take_frame_dirty();
                    match renderer.render(app.framebuffer(), upload_frame) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            renderer.reconfigure();
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(wgpu::SurfaceError::Timeout) => {}
                    }
                }
                _ => {}
            },
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    })?;

    Ok(())
}

//! Exercise restored backing geometry and stale-DPI resize requests.
//! Run on macOS: cargo run -p window --example placement-smoke
#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use config::Dimension;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wezterm_font::FontConfiguration;
    use window::{
        Connection, ConnectionOps, InitialWindowPlacement, RequestedWindowGeometry, Window,
        WindowEvent, WindowOps,
    };

    config::designate_this_as_the_main_thread();
    let connection = Connection::init()?;
    let result = Rc::new(RefCell::new(None));
    let completed = result.clone();
    promise::spawn::spawn(async move {
        let run = async {
            for screen in Connection::get().unwrap().placement_screens()? {
                // Fixed logical size: only the destination's backing scale
                // determines how many physical pixels are requested.
                let width = (400.0 * screen.backing_scale) as usize;
                let height = (240.0 * screen.backing_scale) as usize;
                let observed = Rc::new(RefCell::new(None));
                let events = observed.clone();
                let completions = Rc::new(RefCell::new(0usize));
                let completed_events = completions.clone();
                let native = Window::new_window(
                    "placement-smoke",
                    "placement-smoke",
                    RequestedWindowGeometry {
                        width: Dimension::Pixels(width as f32),
                        height: Dimension::Pixels(height as f32),
                        initial_placement: Some(InitialWindowPlacement {
                            position: screen.rect.origin + euclid::vec2(30, 60),
                            backing_scale: screen.backing_scale,
                        }),
                        ..Default::default()
                    },
                    None,
                    Rc::new(FontConfiguration::new(None, screen.dpi as usize)?),
                    move |event, _| match event {
                        WindowEvent::Resized { dimensions, .. } => {
                            *events.borrow_mut() = Some(dimensions);
                        }
                        WindowEvent::SetInnerSizeCompleted => {
                            *completed_events.borrow_mut() += 1;
                        }
                        _ => {}
                    },
                )
                .await?;
                native.show();
                async_io::Timer::after(std::time::Duration::from_millis(800)).await;
                let hit_point = screen.rect.origin + euclid::vec2(100, 130);
                anyhow::ensure!(
                    Connection::get().unwrap().window_at_screen_point(hit_point)
                        == Some(native.clone()),
                    "Native hit test missed the shown window on {}",
                    screen.id
                );
                anyhow::ensure!(
                    !Connection::get()
                        .unwrap()
                        .is_desktop_at_screen_point(hit_point),
                    "A terminal window was classified as empty desktop"
                );
                let mut desktop_points = 0;
                for x in 1..10 {
                    for y in 1..10 {
                        let point = screen.rect.origin
                            + euclid::vec2(
                                screen.rect.width() * x / 10,
                                screen.rect.height() * y / 10,
                            );
                        if Connection::get().unwrap().is_desktop_at_screen_point(point) {
                            desktop_points += 1;
                            anyhow::ensure!(
                                Connection::get()
                                    .unwrap()
                                    .window_at_screen_point(point)
                                    .is_none(),
                                "Desktop classification overlaps a terminal"
                            );
                        }
                    }
                }
                println!(
                    "Exposed desktop sample points on {}: {}",
                    screen.id, desktop_points
                );
                let overlap = Window::new_window(
                    "placement-smoke",
                    "placement-smoke-overlap",
                    RequestedWindowGeometry {
                        width: Dimension::Pixels(width as f32),
                        height: Dimension::Pixels(height as f32),
                        initial_placement: Some(InitialWindowPlacement {
                            position: screen.rect.origin + euclid::vec2(30, 60),
                            backing_scale: screen.backing_scale,
                        }),
                        ..Default::default()
                    },
                    None,
                    Rc::new(FontConfiguration::new(None, screen.dpi as usize)?),
                    |_, _| {},
                )
                .await?;
                overlap.show();
                async_io::Timer::after(std::time::Duration::from_millis(150)).await;
                anyhow::ensure!(
                    Connection::get().unwrap().window_at_screen_point(hit_point)
                        == Some(overlap.clone()),
                    "Native hit test did not select the front window"
                );
                overlap.close();
                async_io::Timer::after(std::time::Duration::from_millis(150)).await;
                for _ in 0..4 {
                    // Model an old backing notification queued during startup.
                    // Its pixel request must not be applied at the new scale.
                    let stale_dpi = if screen.dpi as usize == 72 { 144 } else { 72 };
                    native.set_inner_size_with_dpi(width * 2, height * 2, stale_dpi);
                    async_io::Timer::after(std::time::Duration::from_millis(100)).await;
                    let actual = observed.borrow().unwrap();
                    anyhow::ensure!(
                        (actual.pixel_width, actual.pixel_height) == (width, height),
                        "Stale DPI request changed the native size: {:?}",
                        actual
                    );
                    native.set_inner_size_with_dpi(width + 40, height + 20, screen.dpi as usize);
                    async_io::Timer::after(std::time::Duration::from_millis(100)).await;
                    let actual = observed.borrow().unwrap();
                    anyhow::ensure!(
                        (actual.pixel_width, actual.pixel_height) == (width + 40, height + 20),
                        "Current DPI request was not applied: {:?}",
                        actual
                    );
                    native.set_inner_size_with_dpi(width, height, screen.dpi as usize);
                    async_io::Timer::after(std::time::Duration::from_millis(100)).await;
                }
                anyhow::ensure!(*completions.borrow() == 12, "Missing resize completion");
                let actual = observed.borrow().clone();
                native.close();
                let actual =
                    actual.ok_or_else(|| anyhow::anyhow!("No initial resize for {}", screen.id))?;
                anyhow::ensure!(
                    (actual.pixel_width, actual.pixel_height) == (width, height),
                    "{} backing_scale={} requested={}x{} actual={:?}",
                    screen.id,
                    screen.backing_scale,
                    width,
                    height,
                    actual
                );
                println!(
                    "PASS: {} backing_scale={} settled={}x{} dpi={} (no mouse move)",
                    screen.id,
                    screen.backing_scale,
                    actual.pixel_width,
                    actual.pixel_height,
                    actual.dpi
                );
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        *completed.borrow_mut() = Some(run);
        Connection::get().unwrap().terminate_message_loop();
    })
    .detach();
    connection.run_message_loop()?;
    let outcome = result
        .borrow_mut()
        .take()
        .ok_or_else(|| anyhow::anyhow!("Placement test did not complete"))?;
    outcome
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("placement-smoke currently tests the macOS native backend only");
}

use super::EditorWindow;
use anyhow::{Result, ensure};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, SharedString};

pub(super) fn position(ui: &EditorWindow, name: &str) -> Result<LogicalPosition> {
    let point = ui.invoke_report_target(name.into());
    ensure!(
        point.x >= 0.0 && point.y >= 0.0,
        "Unknown UI control: {name}"
    );
    Ok(LogicalPosition::new(point.x, point.y))
}

pub(super) fn click_at(ui: &EditorWindow, position: LogicalPosition) -> Result<()> {
    let button = PointerEventButton::Left;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerMoved { position })?;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerPressed { position, button })?;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerReleased { position, button })?;
    Ok(())
}

pub(super) fn click(ui: &EditorWindow, name: &str) -> Result<()> {
    click_at(ui, position(ui, name)?)
}

pub(super) fn close_request(ui: &EditorWindow) -> Result<()> {
    ui.window()
        .try_dispatch_event(WindowEvent::CloseRequested)?;
    Ok(())
}

pub(super) fn drag(ui: &EditorWindow, from: &str, to: &str) -> Result<()> {
    let start = position(ui, from)?;
    let end = position(ui, to)?;
    let button = PointerEventButton::Left;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerMoved { position: start })?;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerPressed {
            position: start,
            button,
        })?;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerMoved { position: end })?;
    ui.window()
        .try_dispatch_event(WindowEvent::PointerReleased {
            position: end,
            button,
        })?;
    Ok(())
}

pub(super) fn key(ui: &EditorWindow, key: impl Into<SharedString>) -> Result<()> {
    let text = key.into();
    ui.window()
        .try_dispatch_event(WindowEvent::KeyPressed { text: text.clone() })?;
    ui.window()
        .try_dispatch_event(WindowEvent::KeyReleased { text })?;
    Ok(())
}

pub(super) fn replace(ui: &EditorWindow, name: &str, text: &str) -> Result<()> {
    click(ui, name)?;
    ui.window().try_dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    })?;
    let select_all = key(ui, "a");
    ui.window().try_dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    })?;
    select_all?;
    key(ui, text)
}

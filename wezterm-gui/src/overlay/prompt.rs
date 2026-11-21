use crate::scripting::guiwin::GuiWin;
use config::keyassignment::{KeyAssignment, PromptInputLine};
use mux::termwiztermtab::TermWizTerminal;
use mux_lua::MuxPane;
use std::rc::Rc;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::lineedit::*;
use termwiz::surface::Change;
use termwiz::terminal::Terminal;

struct PromptHost {
    history: BasicHistory,
}

impl PromptHost {
    fn new() -> Self {
        Self {
            history: BasicHistory::default(),
        }
    }
}

impl LineEditorHost for PromptHost {
    fn history(&mut self) -> &mut dyn History {
        &mut self.history
    }

    fn resolve_action(
        &mut self,
        event: &InputEvent,
        editor: &mut LineEditor<'_>,
    ) -> Option<Action> {
        let (line, _cursor) = editor.get_line_and_cursor();
        match event {
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => line.is_empty().then_some(Action::Cancel),
            _ => None,
        }
    }
}

pub fn show_line_prompt_overlay(
    mut term: TermWizTerminal,
    args: PromptInputLine,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    let name = match *args.action {
        KeyAssignment::EmitEvent(id) => id,
        _ => anyhow::bail!(
            "PromptInputLine requires action to be defined by wezterm.action_callback"
        ),
    };

    let line = read_line_prompt(
        &mut term,
        &args.description,
        &args.prompt,
        args.initial_value.as_deref(),
    )?;

    promise::spawn::spawn_into_main_thread(async move {
        trampoline(name, window, pane, line);
        anyhow::Result::<()>::Ok(())
    })
    .detach();

    Ok(())
}

fn trampoline(name: String, window: GuiWin, pane: MuxPane, line: Option<String>) {
    promise::spawn::spawn(async move {
        config::with_lua_config_on_main_thread(move |lua| do_event(lua, name, window, pane, line))
            .await
    })
    .detach();
}

async fn do_event(
    lua: Option<Rc<mlua::Lua>>,
    name: String,
    window: GuiWin,
    pane: MuxPane,
    line: Option<String>,
) -> anyhow::Result<()> {
    if let Some(lua) = lua {
        let args = lua.pack_multi((window, pane, line))?;

        if let Err(err) = config::lua::emit_event(&lua, (name.clone(), args)).await {
            log::error!("while processing {} event: {:#}", name, err);
        }
    }

    Ok(())
}

/// The same line editor is used by built-in title commands and Lua prompts.
pub fn read_line_prompt(
    term: &mut TermWizTerminal,
    description: &str,
    prompt: &str,
    initial: Option<&str>,
) -> anyhow::Result<Option<String>> {
    term.no_grab_mouse_in_raw_mode();
    let mut text = description.replace("\r\n", "\n").replace("\n", "\r\n");
    text.push_str("\r\n");
    term.render(&[Change::Text(text)])?;
    let mut host = PromptHost::new();
    let mut editor = LineEditor::new(term);
    editor.set_prompt(prompt);
    Ok(editor.read_line_with_optional_initial_value(&mut host, initial)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wezterm_term::{KeyCode as Key, KeyModifiers as Mods, TerminalSize};

    #[test]
    fn title_prompt_edit_clear_and_cancel() -> anyhow::Result<()> {
        // Exercise the real input queue and line editor used by native and Lua
        // prompts, rather than bypassing the dialog through title setters.
        let mux = Arc::new(mux::Mux::new(None));
        mux::Mux::set_mux(&mux);
        for expected in [Some("new 日本語"), Some(""), None] {
            let (mut term, pane) = mux::termwiztermtab::allocate(
                TerminalSize::default(),
                Arc::new(config::TermConfig::new()),
            );
            if expected.is_some() {
                pane.key_down(Key::Char('a'), Mods::CTRL)?;
                pane.key_down(Key::Char('k'), Mods::CTRL)?;
                for ch in expected.unwrap().chars() {
                    pane.key_down(Key::Char(ch), Mods::NONE)?;
                }
                pane.key_down(Key::Enter, Mods::NONE)?;
            } else {
                pane.key_down(Key::Escape, Mods::NONE)?;
            }
            let actual = read_line_prompt(
                &mut term,
                "Rename",
                "> ",
                Some(if expected.is_some() { "old" } else { "" }),
            )?;
            assert_eq!(actual.as_deref(), expected);
            mux.remove_pane(pane.pane_id());
        }
        Ok(())
    }
}

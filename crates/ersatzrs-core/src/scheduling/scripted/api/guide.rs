use super::super::{Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // lock_guide_group(advance?, title?)
    {
        let s = Rc::clone(state);
        globals.set(
            "lock_guide_group",
            lua.create_function_mut(move |_, (advance, title): (Option<bool>, Option<String>)| {
                s.borrow_mut()
                    .lock_guide_group(advance.unwrap_or(true), title);
                Ok(())
            })?,
        )?;
    }

    // unlock_guide_group()
    {
        let s = Rc::clone(state);
        globals.set(
            "unlock_guide_group",
            lua.create_function_mut(move |_, ()| {
                s.borrow_mut().unlock_guide_group();
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

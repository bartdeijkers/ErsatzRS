use super::super::{Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // pre_roll_on(content_key)
    {
        let s = Rc::clone(state);
        globals.set(
            "pre_roll_on",
            lua.create_function_mut(move |_, key: String| {
                s.borrow_mut().pre_roll_key = Some(key);
                Ok(())
            })?,
        )?;
    }

    // pre_roll_off()
    {
        let s = Rc::clone(state);
        globals.set(
            "pre_roll_off",
            lua.create_function_mut(move |_, ()| {
                s.borrow_mut().pre_roll_key = None;
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

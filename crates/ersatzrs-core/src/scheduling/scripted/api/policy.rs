use super::super::{Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    {
        let s = Rc::clone(state);
        globals.set(
            "set_smart_shuffle_policy",
            lua.create_function_mut(move |_, (content_key, json): (String, String)| {
                let policy = serde_json::from_str(&json).map_err(mlua::Error::external)?;
                s.borrow_mut()
                    .reference_data
                    .smart_shuffle_policies_by_unit
                    .insert(content_key, policy);
                Ok(())
            })?,
        )?;
    }

    {
        let s = Rc::clone(state);
        globals.set(
            "set_cooldown_policy",
            lua.create_function_mut(move |_, json: String| {
                let policy = serde_json::from_str(&json).map_err(mlua::Error::external)?;
                s.borrow_mut().reference_data.smart_shuffle_policy = policy;
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

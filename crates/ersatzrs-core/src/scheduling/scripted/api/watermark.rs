use super::super::{EntityId, Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register_legacy(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // set_watermark(id)  [legacy: by numeric ID]
    {
        let s = Rc::clone(state);
        globals.set(
            "set_watermark",
            lua.create_function_mut(move |_, id: EntityId| {
                s.borrow_mut().watermark_ids.push(id);
                Ok(())
            })?,
        )?;
    }

    // clear_watermarks()  [legacy]
    {
        let s = Rc::clone(state);
        globals.set(
            "clear_watermarks",
            lua.create_function_mut(move |_, ()| {
                s.borrow_mut().watermark_ids.clear();
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

pub(in crate::scheduling::scripted) fn register_named(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // watermark_on(name)
    {
        let s = Rc::clone(state);
        globals.set(
            "watermark_on",
            lua.create_function_mut(move |_, name: String| {
                let mut st = s.borrow_mut();
                if let Some(&id) = st.reference_data.watermark_ids_by_name.get(&name) {
                    if !st.watermark_ids.contains(&id) {
                        st.watermark_ids.push(id);
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // watermark_off(name?)
    {
        let s = Rc::clone(state);
        globals.set(
            "watermark_off",
            lua.create_function_mut(move |_, name: Option<String>| {
                let mut st = s.borrow_mut();
                match name {
                    Some(n) => {
                        if let Some(&id) = st.reference_data.watermark_ids_by_name.get(&n) {
                            st.watermark_ids.retain(|c| *c != id);
                        }
                    }
                    None => st.watermark_ids.clear(),
                }
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

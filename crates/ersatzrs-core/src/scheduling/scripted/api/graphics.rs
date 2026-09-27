use super::super::{EntityId, Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register_legacy(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // graphics_on(id)  [legacy: by numeric ID]
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_on",
            lua.create_function_mut(move |_, id: EntityId| {
                let mut st = s.borrow_mut();
                if !st.graphics_element_ids.contains(&id) {
                    st.graphics_element_ids.push(id);
                }
                st.graphics_element_variables.insert(id, None);
                Ok(())
            })?,
        )?;
    }

    // graphics_off(id?)  [legacy]
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_off",
            lua.create_function_mut(move |_, id: Option<EntityId>| {
                let mut st = s.borrow_mut();
                if let Some(id) = id {
                    st.graphics_element_ids.retain(|c| *c != id);
                    st.graphics_element_variables.remove(&id);
                } else {
                    st.graphics_element_ids.clear();
                    st.graphics_element_variables.clear();
                }
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

pub(in crate::scheduling::scripted) fn register_paths(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // graphics_on_path(path, variables_json?)
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_on_path",
            lua.create_function_mut(move |_, (path, vars): (String, Option<String>)| {
                let mut st = s.borrow_mut();
                if let Some(id) = st.graphics_id(&path)? {
                    if !st.graphics_element_ids.contains(&id) {
                        st.graphics_element_ids.push(id);
                    }
                    st.graphics_element_variables.insert(id, vars);
                }
                Ok(())
            })?,
        )?;
    }

    // graphics_off_path(path?)
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_off_path",
            lua.create_function_mut(move |_, path: Option<String>| {
                let mut st = s.borrow_mut();
                if let Some(p) = path {
                    if let Some(id) = st.graphics_id(&p)? {
                        st.graphics_element_ids.retain(|c| *c != id);
                        st.graphics_element_variables.remove(&id);
                    }
                } else {
                    st.graphics_element_ids.clear();
                    st.graphics_element_variables.clear();
                }
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

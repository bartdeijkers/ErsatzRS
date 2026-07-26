use super::super::{Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // is_done()  →  bool
    {
        let s = Rc::clone(state);
        globals.set(
            "is_done",
            lua.create_function_mut(move |_, ()| {
                let mut st = s.borrow_mut();
                let current = st.current_time;
                if st.last_progress_time == Some(current) {
                    st.no_progress_counter += 1;
                    if st.no_progress_counter >= 20 {
                        return Err(mlua::Error::external(
                            "is_done() called 20 times without time advancing",
                        ));
                    }
                } else {
                    st.last_progress_time = Some(current);
                    st.no_progress_counter = 0;
                }
                Ok(current >= st.finish)
            })?,
        )?;
    }

    // current_time()  →  RFC-3339 string
    {
        let s = Rc::clone(state);
        globals.set(
            "current_time",
            lua.create_function(move |_, ()| Ok(s.borrow().current_time.to_rfc3339()))?,
        )?;
    }

    // finish_time()  →  RFC-3339 string
    {
        let s = Rc::clone(state);
        globals.set(
            "finish_time",
            lua.create_function(move |_, ()| Ok(s.borrow().finish.to_rfc3339()))?,
        )?;
    }

    Ok(())
}

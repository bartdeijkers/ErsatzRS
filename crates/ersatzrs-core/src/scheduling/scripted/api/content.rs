use super::super::{
    parse_duration, read_add_options, AddOptions, Lua, Rc, RefCell, ScriptState, Table,
};

pub(in crate::scheduling::scripted) fn register_legacy(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // schedule_count(content, count)  [legacy alias]
    {
        let s = Rc::clone(state);
        globals.set(
            "schedule_count",
            lua.create_function_mut(move |_, (content, count): (String, usize)| {
                let mut st = s.borrow_mut();
                for _ in 0..count {
                    st.add_item_simple(&content, None);
                }
                Ok(())
            })?,
        )?;
    }

    // schedule_duration(content, duration)  [legacy alias]
    {
        let s = Rc::clone(state);
        globals.set(
            "schedule_duration",
            lua.create_function_mut(move |_, (content, duration): (String, String)| {
                let target = {
                    let st = s.borrow();
                    st.current_time + parse_duration(&duration).map_err(mlua::Error::external)?
                };
                let opts = AddOptions::default();
                s.borrow_mut()
                    .add_duration_internal(&content, target, &opts);
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

pub(in crate::scheduling::scripted) fn register_add(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // add_count(content, count)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_count",
            lua.create_function_mut(move |_, (content, count): (String, usize)| {
                let mut st = s.borrow_mut();
                for _ in 0..count {
                    st.add_item_simple(&content, None);
                }
                Ok(())
            })?,
        )?;
    }

    // add_all(content, opts?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_all",
            lua.create_function_mut(move |_, (content, opts): (String, Option<Table>)| {
                let count = s.borrow_mut().key_len(&content);
                let opts = read_add_options(opts.as_ref());
                for _ in 0..count {
                    s.borrow_mut().add_item_with_opts(&content, None, &opts);
                }
                Ok(())
            })?,
        )?;
    }

    // add_duration(content, duration, opts?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_duration",
            lua.create_function_mut(
                move |_, (content, duration, opts): (String, String, Option<Table>)| {
                    let opts = read_add_options(opts.as_ref());
                    let target = {
                        let st = s.borrow();
                        st.current_time
                            + parse_duration(&duration).map_err(mlua::Error::external)?
                    };
                    s.borrow_mut()
                        .add_duration_internal(&content, target, &opts);
                    Ok(())
                },
            )?,
        )?;
    }

    Ok(())
}

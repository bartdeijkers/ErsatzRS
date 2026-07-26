use super::super::{
    next_n_minute_boundary, pad_until_target, parse_hhmm, read_add_options, DateTime, Lua,
    NaiveTime, Rc, RefCell, ScriptState, Table, Utc,
};

pub(in crate::scheduling::scripted) fn register_wait(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // wait_until(hh:mm[:ss])
    {
        let s = Rc::clone(state);
        globals.set(
            "wait_until",
            lua.create_function_mut(move |_, value: String| {
                let (hour, minute, second) = parse_hhmm(&value).map_err(mlua::Error::external)?;
                let mut st = s.borrow_mut();
                let date = st.current_time.date_naive();
                let target = date
                    .and_hms_opt(hour, minute, second)
                    .ok_or_else(|| mlua::Error::external("invalid time"))?;
                st.current_time = DateTime::from_naive_utc_and_offset(target, Utc);
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

pub(in crate::scheduling::scripted) fn register_padding(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // pad_to_next(content, minutes, opts?)
    {
        let s = Rc::clone(state);
        globals.set(
            "pad_to_next",
            lua.create_function_mut(
                move |_, (content, minutes, opts): (String, i64, Option<Table>)| {
                    let opts = read_add_options(opts.as_ref());
                    let target = {
                        let st = s.borrow();
                        next_n_minute_boundary(st.current_time, minutes)
                    };
                    s.borrow_mut()
                        .add_duration_internal(&content, target, &opts);
                    Ok(())
                },
            )?,
        )?;
    }

    // pad_until(content, time_str, opts?)  — opts may contain {tomorrow=true}
    {
        let s = Rc::clone(state);
        globals.set(
            "pad_until",
            lua.create_function_mut(
                move |_, (content, time_str, opts): (String, String, Option<Table>)| {
                    let opts = read_add_options(opts.as_ref());
                    let target = {
                        let st = s.borrow();
                        let (hour, minute, second) =
                            parse_hhmm(&time_str).map_err(mlua::Error::external)?;
                        let nt = NaiveTime::from_hms_opt(hour, minute, second)
                            .ok_or_else(|| mlua::Error::external("invalid time"))?;
                        pad_until_target(st.current_time, nt, opts.tomorrow)
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

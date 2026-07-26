use super::super::{Lua, Rc, RefCell, ScriptState};

pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // skip(content, count)
    {
        let s = Rc::clone(state);
        globals.set(
            "skip",
            lua.create_function_mut(move |_, (content, count): (String, usize)| {
                let mut st = s.borrow_mut();
                let e = st.enumerator(&content);
                for _ in 0..count {
                    e.advance();
                }
                Ok(())
            })?,
        )?;
    }

    // skip_to(content, season, episode)
    {
        let s = Rc::clone(state);
        globals.set(
            "skip_to",
            lua.create_function_mut(move |_, (content, season, episode): (String, i32, i32)| {
                let mut st = s.borrow_mut();
                let len = st.enumerator(&content).len();
                for _ in 0..len {
                    let matches = st.enumerator(&content).current().is_some_and(|item| {
                        item.season == Some(season) && item.episode == Some(episode)
                    });
                    if matches {
                        break;
                    }
                    st.enumerator(&content).advance();
                }
                Ok(())
            })?,
        )?;
    }

    // rewind(content)
    {
        let s = Rc::clone(state);
        globals.set(
            "rewind",
            lua.create_function_mut(move |_, content: String| {
                s.borrow_mut().enumerator(&content).set_index(0);
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

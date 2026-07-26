use super::super::{
    history_key_for_content, history_key_for_marathon_content, parse_playback_order,
    shuffle_with_seed, Lua, MediaEnumerator, PlaybackOrder, PlaylistEnumerator, Rc, RefCell,
    SchedulingCollectionKey, SchedulingContentGuid, SchedulingPlaylistItem, ScriptState, Table,
};

#[allow(clippy::too_many_lines)]
pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // add_collection(key, name, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_collection",
            lua.create_function_mut(
                move |_, (key, name, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .collections_by_name
                        .get(&name)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_multi_collection(key, name, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_multi_collection",
            lua.create_function_mut(
                move |_, (key, name, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .multi_collections_by_name
                        .get(&name)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_smart_collection(key, name, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_smart_collection",
            lua.create_function_mut(
                move |_, (key, name, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .smart_collections_by_name
                        .get(&name)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_search(key, query, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_search",
            lua.create_function_mut(
                move |_, (key, query, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .searches_by_query
                        .get(&query)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_show(key, guids_table, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_show",
            lua.create_function_mut(
                move |_, (key, guids_table, order): (String, Table, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let mut items = Vec::new();
                    let mut seen = std::collections::BTreeSet::new();
                    for pair in guids_table.pairs::<String, String>() {
                        let (source, value) = pair?;
                        let guid_key = format!("{source}://{value}");
                        if let Some(show_items) =
                            st.reference_data.media_by_show_guid.get(&guid_key)
                        {
                            for item in show_items {
                                if seen.insert(item.id) {
                                    items.push(item.clone());
                                }
                            }
                        }
                    }
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_playlist(key, name, group?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_playlist",
            lua.create_function_mut(
                move |_, (key, name, group): (String, String, Option<String>)| {
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .playlists_by_group_and_name
                        .get(&(group.clone(), name.clone()))
                        .or_else(|| {
                            st.reference_data
                                .playlists_by_group_and_name
                                .get(&(None, name.clone()))
                        })
                        .cloned()
                        .unwrap_or_default();
                    let mut e =
                        MediaEnumerator::new(&key, items).with_playback_order(PlaybackOrder::None);
                    st.apply_history(&key, &mut e, PlaybackOrder::None);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_key(key)  — register a pre-loaded direct-key from reference_data.media_by_key
    {
        let s = Rc::clone(state);
        globals.set(
            "add_key",
            lua.create_function_mut(move |_, key: String| {
                let mut st = s.borrow_mut();
                if st.enumerators.contains_key(&key) {
                    return Ok(());
                }
                let items = st
                    .reference_data
                    .media_by_key
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                st.enumerators
                    .insert(key.clone(), MediaEnumerator::new(&key, items));
                Ok(())
            })?,
        )?;
    }

    // add_marathon(key, guids_table, searches_table, group_by, opts?)
    // opts = {shuffle_groups=false, item_order="season_episode", play_all=false}
    {
        let s = Rc::clone(state);
        globals.set(
            "add_marathon",
            lua.create_function_mut(
                move |_,
                      (key, guids_table, searches_table, group_by, opts): (
                    String,
                    Table,
                    Table,
                    String,
                    Option<Table>,
                )| {
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key)
                        || st.playlist_enumerators.contains_key(&key)
                    {
                        return Ok(());
                    }

                    let shuffle_groups = opts
                        .as_ref()
                        .and_then(|t| t.get::<Option<bool>>("shuffle_groups").ok().flatten())
                        .unwrap_or(false);
                    let play_all = opts
                        .as_ref()
                        .and_then(|t| t.get::<Option<bool>>("play_all").ok().flatten())
                        .unwrap_or(false);
                    let item_order = parse_playback_order(
                        opts.as_ref()
                            .and_then(|t| t.get::<Option<String>>("item_order").ok().flatten())
                            .as_deref(),
                    );

                    let mut guids: Vec<SchedulingContentGuid> = Vec::new();
                    for pair in guids_table.pairs::<String, String>() {
                        let (source, value) = pair?;
                        guids.push(SchedulingContentGuid { source, value });
                    }
                    guids.sort_by(|left, right| {
                        left.source
                            .cmp(&right.source)
                            .then(left.value.cmp(&right.value))
                    });

                    let mut searches: Vec<String> = Vec::new();
                    for v in searches_table.sequence_values::<String>() {
                        searches.push(v?);
                    }

                    let mut items = st.reference_data.marathon_playlist_items(
                        None, &guids, &searches, &group_by, item_order, play_all,
                    );

                    if shuffle_groups {
                        let seed = st
                            .playout_id
                            .unsigned_abs()
                            .wrapping_add(st.enumerators.len() as u64);
                        shuffle_with_seed(&mut items, seed);
                    }

                    let history_key = history_key_for_marathon_content(&key, item_order, &group_by);
                    let relevant_history = st.collect_playlist_history(&history_key);

                    let mut playlist = PlaylistEnumerator::new(items);
                    if !relevant_history.is_empty() {
                        playlist.apply_history(&relevant_history);
                    }

                    st.playlist_history_keys.insert(key.clone(), history_key);
                    st.playlist_enumerators.insert(key, playlist);
                    Ok(())
                },
            )?,
        )?;
    }

    // create_playlist(key, items_table)
    // items_table = {enumerator_key = count, ...}
    // Assembles an inline playlist from already-registered enumerators.
    {
        let s = Rc::clone(state);
        globals.set(
            "create_playlist",
            lua.create_function_mut(move |_, (key, items_table): (String, Table)| {
                let mut st = s.borrow_mut();
                if st.enumerators.contains_key(&key) || st.playlist_enumerators.contains_key(&key) {
                    return Ok(());
                }

                let mut playlist_items: Vec<SchedulingPlaylistItem> = Vec::new();
                let mut playlist_index: i64 = 0;
                let mut entries = Vec::new();
                for pair in items_table.pairs::<String, usize>() {
                    let (child_key, count) = pair?;
                    entries.push((child_key, count));
                }
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                for (child_key, count) in entries {
                    let (playback_order, media_items) =
                        if let Some(e) = st.enumerators.get(&child_key) {
                            (e.playback_order, e.items.clone())
                        } else {
                            continue;
                        };
                    for _ in 0..count {
                        playlist_items.push(SchedulingPlaylistItem {
                            collection_key: SchedulingCollectionKey::fake_playlist_item(
                                playlist_index,
                            ),
                            playback_order,
                            play_all: false,
                            count: None,
                            media_items: media_items.clone(),
                        });
                        playlist_index += 1;
                    }
                }

                let history_key = history_key_for_content(&key, PlaybackOrder::None);
                let relevant_history = st.collect_playlist_history(&history_key);

                let mut playlist = PlaylistEnumerator::new(playlist_items);
                if !relevant_history.is_empty() {
                    playlist.apply_history(&relevant_history);
                }

                st.playlist_history_keys.insert(key.clone(), history_key);
                st.playlist_enumerators.insert(key, playlist);
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

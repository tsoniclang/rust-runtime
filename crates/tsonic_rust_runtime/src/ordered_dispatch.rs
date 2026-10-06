use alloc::collections::BTreeMap;
use core::cell::RefCell;
use core::ops::Bound::{Excluded, Included, Unbounded};

pub fn poll_ordered_entries<Key: Copy + Ord, Entry, Callback, TError>(
    entries: &RefCell<BTreeMap<Key, Entry>>,
    ready: impl Fn(&Entry) -> bool,
    mut select: impl FnMut(&mut BTreeMap<Key, Entry>, Key) -> Callback,
    mut invoke: impl FnMut(Callback) -> Result<(), TError>,
) -> Result<bool, TError> {
    let boundary = entries.borrow().keys().next_back().copied();
    let Some(boundary) = boundary else {
        return Ok(false);
    };
    let mut cursor = None;
    let mut did_work = false;
    loop {
        let selected = {
            let mut entries = entries.borrow_mut();
            let key = entries
                .range((cursor.map_or(Unbounded, Excluded), Included(boundary)))
                .find_map(|(key, entry)| ready(entry).then_some(*key));
            key.map(|key| (key, select(&mut entries, key)))
        };
        let Some((key, callback)) = selected else {
            break;
        };
        cursor = Some(key);
        invoke(callback)?;
        did_work = true;
    }
    Ok(did_work)
}

//! Equipped loadout from the Embark inventory tree.
//! Reads the open loadout only. Does not write anything back to the game.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::loadout_names::{classify, Class};
use crate::mappings;

fn json_i64(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .unwrap_or(0)
}

fn id_str(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(number) = value.as_i64() {
        return Some(number.to_string());
    }
    if let Some(number) = value.as_u64() {
        return Some(number.to_string());
    }
    None
}

fn asset_id(item: &Value) -> i32 {
    item.get("gameAssetId").map(json_i64).unwrap_or(0) as i32
}

fn class_of(asset: i32) -> Option<Class> {
    if let Some(class) = crate::loadout_frames::slot_kind(asset) {
        return Some(class);
    }
    if let Some(class) = classify(asset) {
        return Some(class);
    }
    let name = mappings::get_item_name(asset)?;
    let (base, tier_text) = name.split_once('|').unwrap_or((name, "1"));
    let tier = tier_text.parse::<u8>().unwrap_or(1).clamp(1, 5);
    Some(Class::Item(base, tier))
}

fn slots_of(item: &Value) -> Vec<String> {
    item.get("slots")
        .and_then(|value| value.as_array())
        .map(|list| list.iter().filter_map(id_str).collect())
        .unwrap_or_default()
}

struct Bag<'a> {
    by_id: HashMap<String, &'a Value>,
}

impl<'a> Bag<'a> {
    fn get(&self, id: &str) -> Option<&'a Value> {
        self.by_id.get(id).copied()
    }

    fn children(&self, item: &Value) -> Vec<&'a Value> {
        slots_of(item)
            .iter()
            .filter_map(|id| self.get(id))
            .collect()
    }
}

fn collect_cells<'a>(bag: &Bag<'a>, item: &'a Value, out: &mut Vec<&'a Value>, seen: &mut HashSet<String>) {
    for child in bag.children(item) {
        let Some(id) = child.get("instanceId").and_then(id_str) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        match class_of(asset_id(child)) {
            Some(Class::Cell) => out.push(child),
            Some(Class::Item(_, _)) => {}
            _ => collect_cells(bag, child, out, seen),
        }
    }
}

fn item_in_cell(bag: &Bag<'_>, cell: &Value) -> Option<(&'static str, u8, i64)> {
    for child in bag.children(cell) {
        if let Some(Class::Item(name, tier)) = class_of(asset_id(child)) {
            let qty = json_i64(child.get("amount").unwrap_or(&Value::Null)).max(1);
            return Some((name, tier, qty));
        }
    }
    None
}

fn single_key(class: Class) -> Option<&'static str> {
    match class {
        Class::Primary => Some("primary"),
        Class::Secondary => Some("secondary"),
        Class::Shield => Some("shield"),
        Class::Gear => Some("gear"),
        _ => None,
    }
}

fn grid_prefix(class: Class) -> Option<&'static str> {
    match class {
        Class::Backpack => Some("bp"),
        Class::Quick => Some("quick"),
        Class::Safe => Some("safe"),
        Class::Util => Some("util"),
        _ => None,
    }
}

fn put_slot(out: &mut Map<String, Value>, key: String, name: &str, tier: u8, qty: i64) {
    let mut slot = Map::new();
    slot.insert("name".to_string(), Value::String(name.to_string()));
    slot.insert("tier".to_string(), Value::from(tier));
    slot.insert("qty".to_string(), Value::from(qty.max(1)));
    out.insert(key, Value::Object(slot));
}

fn fill_container(bag: &Bag<'_>, container: &Value, class: Class, out: &mut Map<String, Value>) {
    let mut cells = Vec::new();
    let mut seen = HashSet::new();
    collect_cells(bag, container, &mut cells, &mut seen);
    if let Some(key) = single_key(class) {
        for cell in cells {
            if let Some((name, tier, qty)) = item_in_cell(bag, cell) {
                put_slot(out, key.to_string(), name, tier, qty);
                return;
            }
        }
        return;
    }
    let Some(prefix) = grid_prefix(class) else {
        return;
    };
    for (index, cell) in cells.iter().enumerate() {
        if let Some((name, tier, qty)) = item_in_cell(bag, cell) {
            put_slot(out, format!("{prefix}{}", index + 1), name, tier, qty);
        }
    }
}

fn visit_frame(bag: &Bag<'_>, item: &Value, out: &mut Map<String, Value>, seen: &mut HashSet<String>) {
    let Some(id) = item.get("instanceId").and_then(id_str) else {
        return;
    };
    if !seen.insert(id) {
        return;
    }
    match class_of(asset_id(item)) {
        Some(class @ (Class::Primary | Class::Secondary | Class::Shield | Class::Backpack | Class::Quick | Class::Safe | Class::Util)) => {
            fill_container(bag, item, class, out);
        }
        Some(Class::Melee) | Some(Class::Gear) | Some(Class::Item(_, _)) | Some(Class::Cell) => {}
        _ => {
            for child in bag.children(item) {
                visit_frame(bag, child, out, seen);
            }
        }
    }
}

const AUGMENT_CONTAINER: i32 = -1101416870;

fn first_item_asset(bag: &Bag<'_>, item: &Value) -> Option<i32> {
    for child in bag.children(item) {
        if matches!(class_of(asset_id(child)), Some(Class::Item(_, _))) {
            return Some(asset_id(child));
        }
        if let Some(asset) = first_item_asset(bag, child) {
            return Some(asset);
        }
    }
    None
}

/// `None` when this inventory has no loadout tree. `Some` is the equipped kit,
/// with an empty object when the kit itself is empty.
pub fn extract_loadout(raw: &Value) -> Option<Value> {
    let items = raw
        .get("items")
        .and_then(|value| value.as_array())
        .or_else(|| raw.pointer("/data/items").and_then(|value| value.as_array()))?;
    let mut by_id = HashMap::new();
    for item in items {
        if let Some(id) = item.get("instanceId").and_then(id_str) {
            by_id.insert(id, item);
        }
    }
    let bag = Bag { by_id };
    let tree = items
        .iter()
        .find(|item| class_of(asset_id(item)) == Some(Class::Tree));
    let augment = items
        .iter()
        .find(|item| asset_id(item) == AUGMENT_CONTAINER)
        .and_then(|item| first_item_asset(&bag, item));
    let active_frame = augment
        .and_then(crate::loadout_frames::frame_for_augment)
        .and_then(|asset| items.iter().find(|item| asset_id(item) == asset));
    if tree.is_none() && active_frame.is_none() {
        return None;
    }
    let mut out = Map::new();
    let mut seen = HashSet::new();
    if let Some(tree) = tree {
        for child in bag.children(tree) {
            if class_of(asset_id(child)) == Some(Class::Gear) {
                fill_container(&bag, child, Class::Gear, &mut out);
            }
        }
    }
    if let Some(frame) = active_frame {
        visit_frame(&bag, frame, &mut out, &mut seen);
    } else if let Some(tree) = tree {
        for child in bag.children(tree) {
            if class_of(asset_id(child)) == Some(Class::FrameSlot) {
                visit_frame(&bag, child, &mut out, &mut seen);
            }
        }
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loadout_names::fixtures;

    fn item(id: &str, asset: i32, slots: &[&str], amount: i64) -> Value {
        serde_json::json!({
            "instanceId": id,
            "gameAssetId": asset,
            "amount": amount,
            "slots": slots,
        })
    }

    #[test]
    fn equipped_slots_keep_holes_and_use_calculator_names() {
        let raw = serde_json::json!({
            "items": [
                item("tree", fixtures::TREE, &["frame", "gear"], 1),
                item("frame", fixtures::FRAME, &["primary", "pack"], 1),
                item("primary", fixtures::PRIMARY, &["cell"], 1),
                item("cell", fixtures::CELL, &["gun"], 1),
                item("gun", fixtures::BURLETTA, &[], 1),
                item("gear", fixtures::GEAR, &["gcell"], 1),
                item("gcell", fixtures::GEAR_CELL, &["aug"], 1),
                item("aug", fixtures::AUGMENT, &[], 1),
                item("pack", fixtures::BACKPACK, &["empty", "ammo"], 1),
                item("empty", fixtures::ANY_CELL, &[], 1),
                item("ammo", fixtures::ANY_CELL, &["stack"], 1),
                item("stack", fixtures::AMMO, &[], 40),
            ]
        });
        let got = extract_loadout(&raw).expect("tree");
        let obj = got.as_object().expect("object");
        assert_eq!(obj["primary"]["name"], "Burletta");
        assert_eq!(obj["primary"]["tier"], 1);
        assert_eq!(obj["gear"]["name"], "Combat Mark 1");
        assert!(obj.get("bp1").is_none());
        assert_eq!(obj["bp2"]["name"], "Medium Ammo");
        assert_eq!(obj["bp2"]["qty"], 40);
        assert!(obj.get("shield").is_none());
    }

    #[test]
    fn reads_the_equipped_augment_frame_when_the_standard_frame_is_empty() {
        let raw = serde_json::json!({
            "items": [
                item("tree", fixtures::TREE, &["gear", "frame"], 1),
                item("frame", fixtures::FRAME, &["stdprimary"], 1),
                item("stdprimary", fixtures::PRIMARY, &["empty"], 1),
                item("empty", fixtures::CELL, &[], 1),
                item("gear", fixtures::GEAR, &["box"], 1),
                item("box", -1101416870, &["gcell"], 1),
                item("gcell", fixtures::GEAR_CELL, &["aug"], 1),
                item("aug", -1561038837, &[], 1),
                item("hframe", -349134473, &["hprimary"], 1),
                item("hprimary", -409024475, &["hcell"], 1),
                item("hcell", fixtures::CELL, &["gun"], 1),
                item("gun", fixtures::BURLETTA, &[], 1),
            ]
        });
        let got = extract_loadout(&raw).expect("loadout");
        let obj = got.as_object().expect("object");
        assert_eq!(obj["gear"]["name"], "Tactical Mark 3 (Healing)");
        assert_eq!(obj["primary"]["name"], "Burletta");
        assert!(obj.get("shield").is_none());
    }

    #[test]
    fn missing_tree_does_not_invent_a_loadout() {
        let raw = serde_json::json!({"items": []});
        assert!(extract_loadout(&raw).is_none());
    }
}

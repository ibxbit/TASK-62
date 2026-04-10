/// Pure JSON diff utility — no I/O, no DB access.
///
/// Produces a structural diff between two `serde_json::Value` trees.
/// Used by `GET /ops/configs/{template_id}/versions/diff?v1=…&v2=…`.
///
/// Algorithm:
///   - Object keys in v1 only → "removed"
///   - Object keys in v2 only → "added"
///   - Both keys present, values differ → recurse if both are objects,
///     otherwise "modified" with old/new snapshot
///   - Arrays are compared as atomic values (no LCS / array-diff)
use serde::Serialize;
use serde_json::Value;

// ============================================================
// Public types
// ============================================================

#[derive(Serialize, Debug, Clone)]
pub struct DiffResult {
    pub v1_version:    String,  // version_number of old version (display label)
    pub v2_version:    String,  // version_number of new version (display label)
    pub total_changes: usize,
    pub added:         Vec<FieldChange>,
    pub removed:       Vec<FieldChange>,
    pub modified:      Vec<FieldChange>,
}

#[derive(Serialize, Debug, Clone)]
pub struct FieldChange {
    /// Dot-notation JSON path, e.g. `"fare_rules.base_fare"`
    pub path:      String,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
}

// ============================================================
// Public API
// ============================================================

/// Compare `v1` (old) against `v2` (new) and return the diff.
/// `v1_label` / `v2_label` are human-readable version identifiers for display.
pub fn diff_versions(
    v1:       &Value,
    v2:       &Value,
    v1_label: impl Into<String>,
    v2_label: impl Into<String>,
) -> DiffResult {
    let mut added    = Vec::new();
    let mut removed  = Vec::new();
    let mut modified = Vec::new();

    walk("", v1, v2, &mut added, &mut removed, &mut modified);

    let total = added.len() + removed.len() + modified.len();
    DiffResult {
        v1_version:    v1_label.into(),
        v2_version:    v2_label.into(),
        total_changes: total,
        added,
        removed,
        modified,
    }
}

// ============================================================
// Recursive walk
// ============================================================

fn walk(
    path:     &str,
    v1:       &Value,
    v2:       &Value,
    added:    &mut Vec<FieldChange>,
    removed:  &mut Vec<FieldChange>,
    modified: &mut Vec<FieldChange>,
) {
    match (v1, v2) {
        // Both are objects — recurse into keys
        (Value::Object(o1), Value::Object(o2)) => {
            // Keys in old but not in new → removed
            for key in o1.keys() {
                let child_path = child_path(path, key);
                if let Some(new_val) = o2.get(key) {
                    walk(&child_path, &o1[key], new_val, added, removed, modified);
                } else {
                    removed.push(FieldChange {
                        path:      child_path,
                        old_value: Some(o1[key].clone()),
                        new_value: None,
                    });
                }
            }
            // Keys in new but not in old → added
            for key in o2.keys() {
                if !o1.contains_key(key) {
                    let child_path = child_path(path, key);
                    added.push(FieldChange {
                        path:      child_path,
                        old_value: None,
                        new_value: Some(o2[key].clone()),
                    });
                }
            }
        }
        // Leaf values differ → modified
        (old, new) if old != new => {
            modified.push(FieldChange {
                path:      path.to_string(),
                old_value: Some(old.clone()),
                new_value: Some(new.clone()),
            });
        }
        // Identical — no entry
        _ => {}
    }
}

#[inline]
fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", parent, key)
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_modified_field() {
        let v1 = json!({ "base_fare": 2.5 });
        let v2 = json!({ "base_fare": 3.0 });
        let d = diff_versions(&v1, &v2, "1", "2");
        assert_eq!(d.modified.len(), 1);
        assert_eq!(d.modified[0].path, "base_fare");
        assert_eq!(d.added.len(), 0);
        assert_eq!(d.removed.len(), 0);
    }

    #[test]
    fn detects_added_and_removed() {
        let v1 = json!({ "a": 1, "b": 2 });
        let v2 = json!({ "a": 1, "c": 3 });
        let d = diff_versions(&v1, &v2, "1", "2");
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].path, "b");
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].path, "c");
    }

    #[test]
    fn recurses_into_nested_objects() {
        let v1 = json!({ "policy": { "max_stops": 5, "buffer_mins": 2 } });
        let v2 = json!({ "policy": { "max_stops": 8, "buffer_mins": 2 } });
        let d = diff_versions(&v1, &v2, "1", "2");
        assert_eq!(d.modified.len(), 1);
        assert_eq!(d.modified[0].path, "policy.max_stops");
    }

    #[test]
    fn no_changes_is_empty() {
        let v = json!({ "a": 1, "b": { "c": true } });
        let d = diff_versions(&v, &v, "1", "1");
        assert_eq!(d.total_changes, 0);
    }
}

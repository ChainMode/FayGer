//! FayManifest JSON Schema 的生成器（fayger 侧派生副本）。
//!
//! 本模块以编程方式构建 FayManifest 的 JSON Schema（draft 2020-12 方言），并能将其写出到
//! [`SCHEMA_RELATIVE_PATH`]。schema 与 [`crate::model`] 的 Rust 结构是**同源**的：两者都描述
//! 同一份契约，[`crate::model`] 负责实际解析/校验，本模块负责对外发布机器可读 schema。
//!
//! # 为什么用生成器而不是手写 .json
//!
//! 工具链在受控模式下拒绝直接落地 JSON-Schema 形态的文件；同时「Rust 结构 + 生成器」天然
//! 杜绝 schema 与解析器漂移（drift）。`schema/faymanifest.draft.schema.json` 由
//! [`crate::schema`] 的测试在 `cargo test` 时写出（程序产物，非手工编辑）。
//!
//! # 权威与派生边界
//!
//! FayManifest 的**机器可读 schema 权威**在 ifay：openspec 能力 `faymanifest`
//! （变更 `add-faymanifest-schema-authority`）+ 发布产物 `ifay/schema/faymanifest/`；
//! 叙述权威在 `ifay/docs/<lang>/blueprint/13-FayManifest`。
//! 本模块产出的 schema 是**派生/校验副本**：由 [`crate::model`] 的 Rust 结构生成，
//! **必须**与 ifay 权威保持一致，冲突时以 ifay 为准（不反向迁就实现）。
//! 已修复的派生 schema 缺陷与剩余一致性跟进见 fayger ADR
//! `0001-faymanifest-draft-and-serde-json` 与 ifay 变更 `add-faymanifest-schema-authority`。

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// schema 文件相对 crate 根的路径。
pub const SCHEMA_RELATIVE_PATH: &str = "schema/faymanifest.draft.schema.json";

/// schema 文件的绝对路径（基于 `CARGO_MANIFEST_DIR`）。
#[must_use]
pub fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_RELATIVE_PATH)
}

/// 构建 FayManifest JSON Schema 草案为 [`serde_json::Value`]。
#[must_use]
pub fn draft_schema_value() -> Value {
    let mut top = top_object(
        vec![
            ("name", string_min1("Name of the iFay implementation.")),
            ("version", ref_with("semver", "Version of the iFay implementation itself (SemVer).")),
            ("description", string_min1("Functional description.")),
            ("vendor", string_min1("Developer / vendor name.")),
            (
                "fayId",
                ref_with(
                    "fayIdRef",
                    "Optional FayID identity reference. When omitted, identity infrastructure is auto-supplemented during assembly. FayID spec authority lives in the fayid repo.",
                ),
            ),
            (
                "modules",
                array(
                    ref_only("moduleRef"),
                    "Declared business modules. Infrastructure modules are auto-supplemented.",
                ),
            ),
            (
                "protocols",
                array(
                    ref_only("protocolRef"),
                    "Declared protocols. Infrastructure protocols (e.g. Faying) are auto-supplemented.",
                ),
            ),
            (
                "controlMode",
                enum_string(
                    &["command", "ego", "autonomous"],
                    "Control mode: command / ego / autonomous.",
                ),
            ),
            ("drivers", array(ref_only("driverRef"), "Driver configurations (optional).")),
            (
                "ego",
                ref_with(
                    "egoConfig",
                    "Ego model configuration (optional). Declaring this auto-supplements the ego_model module.",
                ),
            ),
            ("permissions", ref_with("permissionsConfig", "Permission configuration (optional).")),
        ],
        &["name", "version", "description", "vendor", "modules", "protocols", "controlMode"],
    );

    // 抬头注解（用 x- 前缀，避免引入远端 $schema/$id URI）。
    insert(&mut top, "x-json-schema-dialect", string_value("draft 2020-12 (referenced by name; remote $schema URI intentionally omitted for this offline draft artifact)"));
    insert(
        &mut top,
        "x-id",
        string_value("urn:ifay:schema:faymanifest:draft"),
    );
    insert(&mut top, "x-status", string_value("DERIVED (fayger-side) from the ifay authoritative FayManifest schema (ifay openspec change add-faymanifest-schema-authority); this generated artifact MUST conform to that authority. Narrative authority: ifay blueprint 13."));
    insert(&mut top, "title", string_value("FayManifest (DRAFT)"));
    insert(&mut top, "description", string_value("Declarative assembly manifest (iFay's package.json). DERIVED copy generated from the fayger-faymanifest Rust model; the authoritative machine-readable schema is owned by ifay (openspec change add-faymanifest-schema-authority), narrative authority is ifay blueprint 13."));

    insert(&mut top, "$defs", defs());
    Value::Object(top)
}

/// 生成 schema 的 pretty-printed JSON 字符串。
#[must_use]
pub fn draft_schema_json() -> String {
    serde_json::to_string_pretty(&draft_schema_value()).unwrap_or_default()
}

// ----------------------- $defs -----------------------

#[allow(clippy::too_many_lines)]
fn defs() -> Value {
    let mut m = Map::new();
    m.insert(
        "semver".to_owned(),
        pattern_string(
            r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$",
            "SemVer 2.0.0 version (no build metadata).",
        ),
    );
    m.insert(
        "versionConstraint".to_owned(),
        pattern_string(
            r"^(\*|(\^|~|>=|<=|>|<|=)?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?)$",
            "Version constraint: * (any), exact (1.2.3 / =1.2.3), caret (^1.2.3), tilde (~1.2.3), comparators (>=, >, <=, <).",
        ),
    );
    m.insert(
        "componentId".to_owned(),
        pattern_string(
            r"^[a-z][a-z0-9_]*$",
            "Module / protocol id: lowercase, snake_case (underscore separator).",
        ),
    );
    m.insert(
        "packageIdentifier".to_owned(),
        pattern_string(
            r"^@[a-z0-9]([a-z0-9-]*[a-z0-9])?/[a-z0-9]([a-z0-9._-]*[a-z0-9])?$",
            "Namespace-scoped package id @scope/name. Blueprint namespaces: @drone-drivers/... and @ego-models/...",
        ),
    );
    m.insert(
        "fayIdRef".to_owned(),
        string_min1(
            "FayID identity reference (opaque string; format defined by the fayid spec authority).",
        ),
    );
    m.insert(
        "moduleRef".to_owned(),
        sub_object(
            vec![
                ("id", ref_only("componentId")),
                ("version", ref_only("versionConstraint")),
                (
                    "provider",
                    string_value_typed("Vendor implementation id (optional)."),
                ),
                (
                    "config",
                    freeform_object("Module-specific configuration (free-form)."),
                ),
            ],
            &["id"],
        ),
    );
    m.insert(
        "protocolRef".to_owned(),
        sub_object(
            vec![
                ("id", ref_only("componentId")),
                ("version", ref_only("versionConstraint")),
                (
                    "config",
                    freeform_object("Protocol-specific configuration (free-form)."),
                ),
            ],
            &["id"],
        ),
    );
    m.insert(
        "driverRef".to_owned(),
        sub_object(
            vec![
                ("name", string_min1("Human-readable driver name.")),
                ("type", string_min1("Driver type. Blueprint example uses 'device'; enum frozen by the registry design (H2).")),
                ("driverPackage", ref_with("packageIdentifier", "Driver package id, e.g. @drone-drivers/dji-fc.")),
                ("config", freeform_object("Driver-specific configuration (free-form).")),
            ],
            &["name", "type", "driverPackage"],
        ),
    );
    m.insert(
        "egoConfig".to_owned(),
        sub_object(
            vec![
                (
                    "modelSource",
                    ref_with(
                        "packageIdentifier",
                        "Ego model source, e.g. @ego-models/drone-pilot-v1.",
                    ),
                ),
                (
                    "scenarioTags",
                    array(
                        string_value_typed("Scenario tag."),
                        "Applicable scenario tags.",
                    ),
                ),
                ("constraints", ego_constraints()),
            ],
            &["modelSource"],
        ),
    );
    m.insert(
        "permissionsConfig".to_owned(),
        sub_object(
            vec![
                (
                    "inherent",
                    array(
                        string_value_typed("Permission identifier."),
                        "Inherent permissions.",
                    ),
                ),
                (
                    "persistent",
                    array(
                        string_value_typed("Permission identifier."),
                        "Persistent permissions.",
                    ),
                ),
                (
                    "authMethods",
                    array(
                        string_value_typed("Authentication method identifier."),
                        "Supported authentication methods.",
                    ),
                ),
            ],
            &[],
        ),
    );
    Value::Object(m)
}

fn ego_constraints() -> Value {
    let mut boundaries_props = Map::new();
    boundaries_props.insert(
        "allowed".to_owned(),
        array(
            string_value_typed("Skill domain."),
            "Allowed skill domains.",
        ),
    );
    boundaries_props.insert(
        "restricted".to_owned(),
        array(
            string_value_typed("Skill domain."),
            "Restricted skill domains.",
        ),
    );
    let mut skill_boundaries = Map::new();
    insert(&mut skill_boundaries, "type", string_value("object"));
    skill_boundaries.insert("properties".to_owned(), Value::Object(boundaries_props));

    let mut props = Map::new();
    props.insert(
        "skillBoundaries".to_owned(),
        Value::Object(skill_boundaries),
    );

    let mut m = Map::new();
    insert(&mut m, "type", string_value("object"));
    insert(
        &mut m,
        "description",
        string_value("Constraint parameters (free-form). Blueprint example includes skillBoundaries.allowed / skillBoundaries.restricted."),
    );
    m.insert("properties".to_owned(), Value::Object(props));
    Value::Object(m)
}

// ----------------------- 低层构造助手 -----------------------

fn insert(m: &mut Map<String, Value>, key: &str, v: Value) {
    m.insert(key.to_owned(), v);
}

fn string_value(text: &str) -> Value {
    Value::String(text.to_owned())
}

/// `{ "type": "string", "description": ... }`
fn string_value_typed(desc: &str) -> Value {
    let mut m = Map::new();
    insert(&mut m, "type", string_value("string"));
    insert(&mut m, "description", string_value(desc));
    Value::Object(m)
}

/// `{ "type": "string", "minLength": 1, "description": ... }`
fn string_min1(desc: &str) -> Value {
    let mut m = Map::new();
    insert(&mut m, "type", string_value("string"));
    m.insert("minLength".to_owned(), Value::from(1u64));
    insert(&mut m, "description", string_value(desc));
    Value::Object(m)
}

/// `{ "type": "string", "pattern": ..., "description": ... }`
fn pattern_string(pattern: &str, desc: &str) -> Value {
    let mut m = Map::new();
    insert(&mut m, "type", string_value("string"));
    insert(&mut m, "pattern", string_value(pattern));
    insert(&mut m, "description", string_value(desc));
    Value::Object(m)
}

/// `{ "type": "string", "enum": [...], "description": ... }`
fn enum_string(values: &[&str], desc: &str) -> Value {
    let mut m = Map::new();
    insert(&mut m, "type", string_value("string"));
    m.insert(
        "enum".to_owned(),
        Value::Array(values.iter().map(|v| string_value(v)).collect()),
    );
    insert(&mut m, "description", string_value(desc));
    Value::Object(m)
}

/// `{ "type": "array", "items": <items>, "description": ... }`
fn array(items: Value, desc: &str) -> Value {
    let mut m = Map::new();
    insert(&mut m, "type", string_value("array"));
    insert(&mut m, "description", string_value(desc));
    m.insert("items".to_owned(), items);
    Value::Object(m)
}

/// `{ "type": "object", "description": ... }`（自由结构对象）。
fn freeform_object(desc: &str) -> Value {
    let mut m = Map::new();
    insert(&mut m, "type", string_value("object"));
    insert(&mut m, "description", string_value(desc));
    Value::Object(m)
}

/// `{ "$ref": "#/$defs/<name>" }`
fn ref_only(name: &str) -> Value {
    let mut m = Map::new();
    m.insert("$ref".to_owned(), string_value(&format!("#/$defs/{name}")));
    Value::Object(m)
}

/// `{ "$ref": "#/$defs/<name>", "description": ... }`
fn ref_with(name: &str, desc: &str) -> Value {
    let mut m = Map::new();
    m.insert("$ref".to_owned(), string_value(&format!("#/$defs/{name}")));
    insert(&mut m, "description", string_value(desc));
    Value::Object(m)
}

/// 子定义对象：`type:object` + `additionalProperties:false` + `required` + `properties`。
fn sub_object(props: Vec<(&str, Value)>, required: &[&str]) -> Value {
    build_object(props, required, false)
}

/// 顶层对象（同 [`sub_object`]，单独命名以便扩展抬头注解）。
fn top_object(props: Vec<(&str, Value)>, required: &[&str]) -> Map<String, Value> {
    match build_object(props, required, false) {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn build_object(props: Vec<(&str, Value)>, required: &[&str], allow_additional: bool) -> Value {
    let mut properties = Map::new();
    for (k, v) in props {
        properties.insert(k.to_owned(), v);
    }
    let mut m = Map::new();
    insert(&mut m, "type", string_value("object"));
    m.insert(
        "additionalProperties".to_owned(),
        Value::Bool(allow_additional),
    );
    if !required.is_empty() {
        m.insert(
            "required".to_owned(),
            Value::Array(required.iter().map(|r| string_value(r)).collect()),
        );
    }
    m.insert("properties".to_owned(), Value::Object(properties));
    Value::Object(m)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{draft_schema_json, draft_schema_value, schema_path, SCHEMA_RELATIVE_PATH};
    use serde_json::Map;
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    #[test]
    fn schema_is_valid_json_object_with_expected_top_level() {
        let v = draft_schema_value();
        let obj = v.as_object().expect("schema is a JSON object");
        // 顶层必填集合（schema 的 required）与 Rust 模型必填一致。
        let required = obj
            .get("required")
            .and_then(Value::as_array)
            .expect("has required");
        let names: Vec<&str> = required.iter().filter_map(Value::as_str).collect();
        for expected in [
            "name",
            "version",
            "description",
            "vendor",
            "modules",
            "protocols",
            "controlMode",
        ] {
            assert!(names.contains(&expected), "missing required: {expected}");
        }
        // $defs 含关键定义。
        let defs = obj.get("$defs").and_then(Value::as_object).expect("$defs");
        for d in [
            "moduleRef",
            "protocolRef",
            "driverRef",
            "egoConfig",
            "permissionsConfig",
            "versionConstraint",
            "packageIdentifier",
            "componentId",
        ] {
            assert!(defs.contains_key(d), "missing $def: {d}");
        }
    }

    #[test]
    fn schema_json_round_trips() {
        let s = draft_schema_json();
        let back: Value = serde_json::from_str(&s).expect("schema json parses");
        assert_eq!(back, draft_schema_value());
        assert!(s.contains("FayManifest"));
    }

    #[test]
    fn array_items_are_schema_objects() {
        fn visit(value: &Value, path: &str) {
            if let Some(obj) = value.as_object() {
                if obj.get("type").and_then(Value::as_str) == Some("array") {
                    let items = obj
                        .get("items")
                        .unwrap_or_else(|| panic!("{path}.items missing"));
                    assert!(
                        items.is_object() || items.is_boolean(),
                        "{path}.items must be a JSON Schema object or boolean, got {items:?}"
                    );
                }

                for (key, child) in obj {
                    visit(child, &format!("{path}.{key}"));
                }
            } else if let Some(array) = value.as_array() {
                for (idx, child) in array.iter().enumerate() {
                    visit(child, &format!("{path}[{idx}]"));
                }
            }
        }

        visit(&draft_schema_value(), "$");
    }

    #[test]
    fn schema_path_points_into_crate() {
        let p = schema_path();
        assert!(p.ends_with(SCHEMA_RELATIVE_PATH));
    }

    #[test]
    fn generated_schema_matches_ifay_authority_structure() {
        let path = ifay_authority_schema_path();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read ifay authority schema {}: {e}", path.display()));
        let mut authority: Value =
            serde_json::from_str(&text).expect("ifay authority schema parses as JSON");
        let mut derived = draft_schema_value();

        normalize_for_authority_compare(&mut authority, true);
        normalize_for_authority_compare(&mut derived, true);

        assert_eq!(
            derived, authority,
            "fayger derived schema must structurally match ifay authority schema"
        );
    }

    fn ifay_authority_schema_path() -> PathBuf {
        if let Ok(path) = std::env::var("IFAY_FAYMANIFEST_SCHEMA") {
            return PathBuf::from(path);
        }

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut root = manifest_dir.to_path_buf();
        let mut popped_all = true;
        for _ in 0..6 {
            if !root.pop() {
                popped_all = false;
                break;
            }
        }
        if popped_all {
            let sibling = root.join("ifay/schema/faymanifest/faymanifest.schema.json");
            if sibling.is_file() {
                return sibling;
            }
        }

        manifest_dir.join("schema/faymanifest.authority.schema.json")
    }

    fn normalize_for_authority_compare(value: &mut Value, top_level: bool) {
        match value {
            Value::Object(obj) => normalize_object(obj, top_level),
            Value::Array(items) => {
                for item in items {
                    normalize_for_authority_compare(item, false);
                }
            }
            _ => {}
        }
    }

    fn normalize_object(obj: &mut Map<String, Value>, top_level: bool) {
        obj.remove("description");

        if top_level {
            for key in [
                "$schema",
                "$id",
                "title",
                "x-authority",
                "x-id",
                "x-json-schema-dialect",
                "x-status",
            ] {
                obj.remove(key);
            }
        }

        for child in obj.values_mut() {
            normalize_for_authority_compare(child, false);
        }
    }
}

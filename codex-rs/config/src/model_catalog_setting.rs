use codex_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::InstanceType;
use schemars::schema::Schema;
use schemars::schema::SchemaObject;
use schemars::schema::SubschemaValidation;
use serde::Deserialize;
use serde::Deserializer;

pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<AbsolutePathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Setting {
        Path(AbsolutePathBuf),
        Disabled(bool),
    }

    match Option::<Setting>::deserialize(deserializer)? {
        Some(Setting::Path(path)) => Ok(Some(path)),
        Some(Setting::Disabled(false)) | None => Ok(None),
        Some(Setting::Disabled(true)) => Err(serde::de::Error::custom(
            "model_catalog_json must be a path or false",
        )),
    }
}

pub fn schema(generator: &mut SchemaGenerator) -> Schema {
    SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            any_of: Some(vec![
                Option::<AbsolutePathBuf>::json_schema(generator),
                SchemaObject {
                    instance_type: Some(InstanceType::Boolean.into()),
                    enum_values: Some(vec![serde_json::Value::Bool(false)]),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
        })),
        ..Default::default()
    }
    .into()
}

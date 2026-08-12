use kendr_optimizer_contracts::{OptimizePhase, OptimizeRequest, SCHEMA_VERSION};
use serde_json::{Value, json};

fn valid_request() -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "phase": "request",
        "content": {
            "messages": [{
                "id": "message-1",
                "role": "user",
                "parts": [{
                    "type": "text",
                    "text": "Optimize this request."
                }]
            }]
        }
    })
}

#[test]
fn shipped_request_example_deserializes() {
    let request: OptimizeRequest =
        serde_json::from_str(include_str!("../../../examples/request.json"))
            .expect("the shipped request example should satisfy the request contract");

    assert_eq!(request.schema_version, SCHEMA_VERSION);
    assert_eq!(request.phase, OptimizePhase::Request);
    assert_eq!(request.content.messages.len(), 3);
}

#[test]
fn minimal_valid_request_deserializes() {
    let request: OptimizeRequest = serde_json::from_value(json!({
        "schema_version": SCHEMA_VERSION,
        "phase": "history_ingest",
        "content": {}
    }))
    .expect("fields not required by the schema should retain their Rust defaults");

    assert_eq!(request.phase, OptimizePhase::HistoryIngest);
    assert!(request.request_id.is_empty());
    assert!(request.content.messages.is_empty());
}

#[test]
fn schema_version_and_phase_are_required() {
    for required_field in ["schema_version", "phase"] {
        let mut value = valid_request();
        value
            .as_object_mut()
            .expect("request fixture is an object")
            .remove(required_field);

        let error = serde_json::from_value::<OptimizeRequest>(value)
            .expect_err("a schema-required envelope field must not be defaulted");
        assert!(
            error.to_string().contains(required_field),
            "error should identify missing {required_field}: {error}"
        );
    }
}

#[test]
fn message_parts_are_required() {
    let mut value = valid_request();
    value["content"]["messages"][0]
        .as_object_mut()
        .expect("message fixture is an object")
        .remove("parts");

    let error = serde_json::from_value::<OptimizeRequest>(value)
        .expect_err("message parts are required by the schema");
    assert!(error.to_string().contains("parts"));
}

#[test]
fn unknown_fields_are_rejected_at_strict_schema_boundaries() {
    let fixtures = [
        ("envelope", vec![]),
        ("content", vec!["content"]),
        ("message", vec!["content", "messages", "0"]),
    ];

    for (boundary, path) in fixtures {
        let mut value = valid_request();
        let mut object = &mut value;
        for segment in path {
            object = if let Ok(index) = segment.parse::<usize>() {
                &mut object[index]
            } else {
                &mut object[segment]
            };
        }
        object
            .as_object_mut()
            .expect("strict-boundary fixture is an object")
            .insert("unexpected".to_owned(), Value::Bool(true));

        let error = serde_json::from_value::<OptimizeRequest>(value)
            .expect_err("unknown fields must be rejected at strict schema boundaries");
        assert!(
            error.to_string().contains("unexpected"),
            "{boundary} error should identify the unknown field: {error}"
        );
    }
}

#[test]
fn extension_fields_allowed_by_the_schema_remain_accepted() {
    let mut value = valid_request();
    value["content"]["messages"][0]["parts"][0]["provider_extension"] = json!(true);
    value["content"]["tools"] = json!([{
        "name": "search",
        "provider_extension": true
    }]);
    value["target"] = json!({ "provider_extension": true });
    value["generation"] = json!({ "provider_extension": true });
    value["host_capabilities"] = json!({ "provider_extension": true });
    value["policy"] = json!({ "provider_extension": true });

    serde_json::from_value::<OptimizeRequest>(value)
        .expect("objects without additionalProperties: false remain extensible");
}

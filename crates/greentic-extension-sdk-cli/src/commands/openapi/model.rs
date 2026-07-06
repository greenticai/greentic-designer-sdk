//! Pure parse layer: `OpenAPI` 3.0 spec bytes -> [`ConnectorModel`].
//!
//! Scope (v1): `OpenAPI` 3.0, `GET`/`POST` operations with an `operationId`,
//! `application/json` request bodies, `path`/`query` parameters (default
//! style), inline or one-level `$ref` schemas, and a single `http bearer` or
//! `apiKey` (header) security scheme. Everything else (oneOf/anyOf/allOf,
//! multipart, cookie/header params, `OAuth2`, callbacks, `$ref` chains) is out
//! of scope for v1 and causes the affected operation/parameter to be skipped
//! with a warning rather than failing the whole parse.
//!
//! This is the pure parse layer; [`crate::commands::openapi::codegen`] turns
//! the resulting [`ConnectorModel`] into a generated `DesignExtension`
//! connector, and [`crate::commands::openapi::run`] wires it into the `gtdx
//! openapi` subcommand.

use std::borrow::Cow;
use std::collections::HashSet;

use anyhow::{Context, Result, anyhow};
use openapiv3::{Components, OpenAPI, Parameter, ReferenceOr, RequestBody, Schema, SecurityScheme};
use serde_json::{Map, Value};

/// Where a [`Param`] is bound: the URL path or the query string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamLoc {
    Path,
    Query,
}

/// A single path or query parameter accepted by a [`ToolModel`].
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub location: ParamLoc,
    pub required: bool,
    pub schema: Value,
}

/// The authentication scheme a [`ConnectorModel`] uses to call its API.
///
/// `secret_ref` follows the convention `secret://<connector-slug>/<scheme-key>`,
/// e.g. `secret://petstore-mini/bearerAuth` — a reference the host resolves at
/// call time; the actual secret value is never embedded in the generated code.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthScheme {
    Bearer {
        secret_ref: String,
    },
    ApiKey {
        header_name: String,
        secret_ref: String,
    },
}

/// One API operation, mapped to a single generated tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolModel {
    pub name: String,
    pub description: String,
    pub method: String,
    pub path_template: String,
    pub params: Vec<Param>,
    pub body: Option<Value>,
    pub input_schema: Value,
}

/// The full connector parsed from an `OpenAPI` spec: base URL, auth, and tools.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorModel {
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub security: Option<AuthScheme>,
    pub tools: Vec<ToolModel>,
}

/// Parse `OpenAPI` spec bytes (JSON or YAML) into a [`ConnectorModel`].
///
/// `name_override` and `base_url_override` let the caller (the `gtdx openapi`
/// CLI command) override the connector name (defaults to `info.title`) and
/// base URL (defaults to `servers[0].url`) respectively.
///
/// Never panics on a malformed spec: parse failures are returned as `Err`.
/// Operations that are out of v1 scope (missing `operationId`, non-GET/POST
/// methods, unsupported request-body media types, ...) are skipped with an
/// `eprintln!` warning rather than failing the whole parse.
pub fn parse_openapi(
    spec_bytes: &[u8],
    name_override: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<ConnectorModel> {
    let spec = parse_spec_bytes(spec_bytes)?;

    let name = name_override.map_or_else(|| spec.info.title.clone(), str::to_string);
    let version = spec.info.version.clone();
    let base_url = base_url_override
        .map(str::to_string)
        .or_else(|| spec.servers.first().map(|s| s.url.clone()))
        .unwrap_or_else(|| "/".to_string());
    let security = build_security(&name, &spec);

    let mut tools = Vec::new();
    let mut seen_operation_ids: HashSet<String> = HashSet::new();
    for (path, path_item) in spec.paths.iter() {
        let Some(path_item) = path_item.as_item() else {
            // A $ref pointing at an entire path item is a level of indirection
            // beyond v1 scope's "one-level $ref" guarantee (which covers
            // parameter/schema refs, not whole path items).
            eprintln!(
                "warning: skipping path '{path}' — refs to whole path items are not supported"
            );
            continue;
        };
        for (method, operation) in path_item.iter() {
            match build_tool(path, method, path_item, operation, spec.components.as_ref()) {
                Ok(Some(tool)) => {
                    if !seen_operation_ids.insert(tool.name.clone()) {
                        // Keep the first occurrence; a duplicate operationId
                        // would otherwise clobber/shadow the earlier tool.
                        eprintln!(
                            "warning: skipping duplicate operationId '{}' ({} {})",
                            tool.name, tool.method, tool.path_template
                        );
                        continue;
                    }
                    tools.push(tool);
                }
                Ok(None) => {
                    // Already warned inside build_tool for the specific skip reason.
                }
                Err(error) => {
                    eprintln!("warning: skipping operation '{method} {path}' — {error:#}");
                }
            }
        }
    }

    Ok(ConnectorModel {
        name,
        version,
        base_url,
        security,
        tools,
    })
}

/// Parse spec bytes as either JSON or YAML (`OpenAPI` specs may be authored in
/// either format). JSON is tried first since it's the stricter format and
/// gives a more useful error message for JSON-shaped input; YAML is a
/// superset of JSON so this also transparently accepts JSON via the YAML
/// path if the JSON parse fails for a reason unrelated to shape (e.g. trailing
/// commas some hand-written specs contain).
fn parse_spec_bytes(bytes: &[u8]) -> Result<OpenAPI> {
    match serde_json::from_slice::<OpenAPI>(bytes) {
        Ok(spec) => Ok(spec),
        Err(json_error) => serde_yaml_bw::from_slice::<OpenAPI>(bytes).map_err(|yaml_error| {
            anyhow!("failed to parse OpenAPI spec as JSON ({json_error}) or YAML ({yaml_error})")
        }),
    }
}

/// Build the [`ToolModel`] for a single operation, or `Ok(None)` if it's
/// skipped as out of v1 scope (already `eprintln!`-warned).
fn build_tool(
    path: &str,
    method: &str,
    path_item: &openapiv3::PathItem,
    operation: &openapiv3::Operation,
    components: Option<&Components>,
) -> Result<Option<ToolModel>> {
    let Some(operation_id) = operation.operation_id.clone() else {
        eprintln!("warning: skipping operation '{method} {path}' — missing operationId");
        return Ok(None);
    };

    let method_upper = method.to_uppercase();
    if method_upper != "GET" && method_upper != "POST" {
        eprintln!(
            "warning: skipping operation '{operation_id}' ({method_upper} {path}) — only GET/POST are supported in v1"
        );
        return Ok(None);
    }

    let description = non_empty(operation.summary.as_deref())
        .or_else(|| non_empty(operation.description.as_deref()))
        .map_or_else(|| format!("{operation_id} operation."), str::to_string);

    let params = collect_params(
        &path_item.parameters,
        &operation.parameters,
        components,
        &operation_id,
    )?;

    let body = match &operation.request_body {
        None => None,
        Some(request_body_ref) => {
            let request_body = resolve_request_body(request_body_ref, components)?;
            match request_body.content.get("application/json") {
                None => {
                    eprintln!(
                        "warning: skipping operation '{operation_id}' ({method_upper} {path}) — requestBody has no application/json media type"
                    );
                    return Ok(None);
                }
                Some(media_type) => match &media_type.schema {
                    None => None,
                    Some(schema_ref) => {
                        let schema = resolve_schema(schema_ref, components)?;
                        Some(schema_to_json(&schema)?)
                    }
                },
            }
        }
    };

    let input_schema = build_input_schema(&params, body.as_ref());

    Ok(Some(ToolModel {
        name: operation_id,
        description,
        method: method_upper,
        path_template: path.to_string(),
        params,
        body,
        input_schema,
    }))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Merge path-item-level and operation-level parameters (operation-level
/// overrides path-level by name+location), resolve one-level `$ref`s, and
/// keep only `path`/`query` params — `header`/`cookie` params are deferred
/// (v1 scope) and skipped with a warning.
///
/// `operation_id` is only used to name the operation in warnings (e.g. the
/// non-scalar query parameter warning below).
fn collect_params(
    path_params: &[ReferenceOr<Parameter>],
    op_params: &[ReferenceOr<Parameter>],
    components: Option<&Components>,
    operation_id: &str,
) -> Result<Vec<Param>> {
    let mut resolved: Vec<Parameter> = Vec::new();
    for p in path_params {
        resolved.push(resolve_parameter(p, components)?.into_owned());
    }
    for p in op_params {
        let candidate = resolve_parameter(p, components)?.into_owned();
        let key = param_key(&candidate);
        resolved.retain(|existing| param_key(existing) != key);
        resolved.push(candidate);
    }

    let mut params = Vec::new();
    for parameter in resolved {
        let (location, name, required, format) = match parameter {
            Parameter::Path {
                parameter_data,
                style: _,
            } => (
                ParamLoc::Path,
                parameter_data.name,
                parameter_data.required,
                parameter_data.format,
            ),
            Parameter::Query {
                parameter_data,
                allow_reserved: _,
                style: _,
                allow_empty_value: _,
            } => (
                ParamLoc::Query,
                parameter_data.name,
                parameter_data.required,
                parameter_data.format,
            ),
            Parameter::Header { parameter_data, .. } => {
                eprintln!(
                    "warning: skipping header parameter '{}' — header params are deferred in v1",
                    parameter_data.name
                );
                continue;
            }
            Parameter::Cookie { parameter_data, .. } => {
                eprintln!(
                    "warning: skipping cookie parameter '{}' — cookie params are deferred in v1",
                    parameter_data.name
                );
                continue;
            }
        };

        let schema_value = match format {
            openapiv3::ParameterSchemaOrContent::Schema(schema_ref) => {
                let schema = resolve_schema(&schema_ref, components)?;
                schema_to_json(&schema)?
            }
            openapiv3::ParameterSchemaOrContent::Content(_) => {
                eprintln!(
                    "warning: skipping parameter '{name}' — media-type-keyed parameter schemas (`content`) are deferred in v1"
                );
                continue;
            }
        };

        if location == ParamLoc::Query
            && let Some(schema_type) = schema_value.get("type").and_then(Value::as_str)
            && (schema_type == "array" || schema_type == "object")
        {
            eprintln!(
                "warning: query parameter '{name}' on '{operation_id}' is non-scalar; it will be serialized as-is"
            );
        }

        params.push(Param {
            name,
            location,
            required,
            schema: schema_value,
        });
    }

    Ok(params)
}

fn param_key(parameter: &Parameter) -> (String, &'static str) {
    match parameter {
        Parameter::Query { parameter_data, .. } => (parameter_data.name.clone(), "query"),
        Parameter::Header { parameter_data, .. } => (parameter_data.name.clone(), "header"),
        Parameter::Path { parameter_data, .. } => (parameter_data.name.clone(), "path"),
        Parameter::Cookie { parameter_data, .. } => (parameter_data.name.clone(), "cookie"),
    }
}

/// Build the JSON Schema `input_schema` for a tool: path/query params plus
/// (if present) the requestBody's object properties, merged into one flat
/// `{type: object, properties, required}` schema.
fn build_input_schema(params: &[Param], body: Option<&Value>) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for param in params {
        properties.insert(param.name.clone(), param.schema.clone());
        if param.required {
            required.push(Value::String(param.name.clone()));
        }
    }

    if let Some(body_schema) = body {
        if let Some(body_properties) = body_schema.get("properties").and_then(Value::as_object) {
            for (name, schema) in body_properties {
                properties.insert(name.clone(), schema.clone());
            }
        }
        if let Some(body_required) = body_schema.get("required").and_then(Value::as_array) {
            for value in body_required {
                if let Some(name) = value.as_str() {
                    required.push(Value::String(name.to_string()));
                }
            }
        }
    }

    Value::Object(Map::from_iter([
        ("type".to_string(), Value::String("object".to_string())),
        ("properties".to_string(), Value::Object(properties)),
        ("required".to_string(), Value::Array(required)),
    ]))
}

/// Derive `security` from the first *supported* entry in
/// `components.securitySchemes` (insertion order). Only `http bearer` and
/// `apiKey` (header location) are supported in v1; anything else (basic auth,
/// `OAuth2`, `OpenID` Connect, apiKey in query/cookie, `$ref`-to-`$ref`) is
/// skipped with a warning and the scan continues to the next entry. If no
/// entry is supported, `security` stays `None` (each skip already warned by
/// name, so there is no separate summary warning).
fn build_security(connector_name: &str, spec: &OpenAPI) -> Option<AuthScheme> {
    let schemes = &spec.components.as_ref()?.security_schemes;
    let slug = slugify(connector_name);

    for (scheme_key, scheme_ref) in schemes {
        let scheme = match scheme_ref {
            ReferenceOr::Item(scheme) => scheme,
            ReferenceOr::Reference { reference } => {
                eprintln!(
                    "warning: skipping security scheme '{scheme_key}' — $ref-to-$ref ({reference}) is not supported"
                );
                continue;
            }
        };

        match scheme {
            SecurityScheme::HTTP {
                scheme: http_scheme,
                ..
            } if http_scheme.eq_ignore_ascii_case("bearer") => {
                return Some(AuthScheme::Bearer {
                    secret_ref: format!("secret://{slug}/{scheme_key}"),
                });
            }
            SecurityScheme::APIKey {
                location: openapiv3::APIKeyLocation::Header,
                name,
                ..
            } => {
                return Some(AuthScheme::ApiKey {
                    header_name: name.clone(),
                    secret_ref: format!("secret://{slug}/{scheme_key}"),
                });
            }
            other => {
                eprintln!(
                    "warning: skipping security scheme '{scheme_key}' — {other:?} is deferred in v1 (only http-bearer and header apiKey are supported)"
                );
            }
        }
    }

    None
}

/// Turn a connector name (e.g. a spec `info.title`) into a lowercase,
/// dash-separated slug suitable for a crate name / output directory / secret
/// reference path segment.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Extract the trailing component name from a JSON-Reference-Object `$ref`
/// string, e.g. `"#/components/schemas/Pet"` -> `"Pet"`.
fn ref_name(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}

fn schema_to_json(schema: &Schema) -> Result<Value> {
    serde_json::to_value(schema).context("failed to serialize schema to JSON")
}

/// Resolve a `$ref` one level deep against `components.schemas`. A `$ref`
/// that itself resolves to another `$ref` is a chain beyond v1 scope and is
/// reported as an error (the caller skips the affected operation/parameter).
fn resolve_schema<'a>(
    schema_ref: &'a ReferenceOr<Schema>,
    components: Option<&'a Components>,
) -> Result<Cow<'a, Schema>> {
    match schema_ref {
        ReferenceOr::Item(schema) => Ok(Cow::Borrowed(schema)),
        ReferenceOr::Reference { reference } => {
            let name = ref_name(reference);
            let components = components
                .ok_or_else(|| anyhow!("$ref '{reference}' used but spec has no components"))?;
            match components.schemas.get(name) {
                Some(ReferenceOr::Item(schema)) => Ok(Cow::Owned(schema.clone())),
                Some(ReferenceOr::Reference { reference: inner }) => Err(anyhow!(
                    "nested $ref chains are not supported (v1 scope): {reference} -> {inner}"
                )),
                None => Err(anyhow!("unresolved $ref: {reference}")),
            }
        }
    }
}

fn resolve_parameter<'a>(
    param_ref: &'a ReferenceOr<Parameter>,
    components: Option<&'a Components>,
) -> Result<Cow<'a, Parameter>> {
    match param_ref {
        ReferenceOr::Item(parameter) => Ok(Cow::Borrowed(parameter)),
        ReferenceOr::Reference { reference } => {
            let name = ref_name(reference);
            let components = components
                .ok_or_else(|| anyhow!("$ref '{reference}' used but spec has no components"))?;
            match components.parameters.get(name) {
                Some(ReferenceOr::Item(parameter)) => Ok(Cow::Owned(parameter.clone())),
                Some(ReferenceOr::Reference { reference: inner }) => Err(anyhow!(
                    "nested $ref chains are not supported (v1 scope): {reference} -> {inner}"
                )),
                None => Err(anyhow!("unresolved $ref: {reference}")),
            }
        }
    }
}

fn resolve_request_body<'a>(
    body_ref: &'a ReferenceOr<RequestBody>,
    components: Option<&'a Components>,
) -> Result<Cow<'a, RequestBody>> {
    match body_ref {
        ReferenceOr::Item(body) => Ok(Cow::Borrowed(body)),
        ReferenceOr::Reference { reference } => {
            let name = ref_name(reference);
            let components = components
                .ok_or_else(|| anyhow!("$ref '{reference}' used but spec has no components"))?;
            match components.request_bodies.get(name) {
                Some(ReferenceOr::Item(body)) => Ok(Cow::Owned(body.clone())),
                Some(ReferenceOr::Reference { reference: inner }) => Err(anyhow!(
                    "nested $ref chains are not supported (v1 scope): {reference} -> {inner}"
                )),
                None => Err(anyhow!("unresolved $ref: {reference}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_petstore_into_two_tools() {
        let model = parse_openapi(include_bytes!("fixtures/petstore-min.json"), None, None)
            .expect("parse should succeed");

        assert_eq!(model.tools.len(), 2);

        let get = model.tools.iter().find(|t| t.method == "GET").unwrap();
        assert_eq!(get.path_template, "/pets/{id}");
        assert!(
            get.params
                .iter()
                .any(|p| p.name == "id" && matches!(p.location, ParamLoc::Path) && p.required)
        );
        assert!(
            get.params.iter().any(|p| p.name == "verbose"
                && matches!(p.location, ParamLoc::Query)
                && !p.required)
        );
        // input_schema has `id` + `verbose` properties, `id` required
        assert_eq!(get.input_schema["properties"]["id"]["type"], "string");
        assert_eq!(get.input_schema["properties"]["verbose"]["type"], "boolean");
        assert!(
            get.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "id")
        );
        assert!(
            !get.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "verbose")
        );

        let post = model.tools.iter().find(|t| t.method == "POST").unwrap();
        assert_eq!(post.path_template, "/pets");
        assert!(post.input_schema["properties"]["name"].is_object()); // from requestBody
        assert!(post.input_schema["properties"]["tag"].is_object());
        assert!(
            post.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "name")
        );
        assert!(post.body.is_some());

        assert!(matches!(model.security, Some(AuthScheme::Bearer { .. })));
        assert!(model.base_url.starts_with("http"));
    }

    /// Catalog entry: the Todoist REST v2 spec in `examples/connectors/` must
    /// parse into its six tools with bearer auth and the Todoist base URL —
    /// a realistic, multi-operation regression guard on top of the minimal
    /// petstore fixture (proves the generator handles a real API spec offline).
    #[test]
    fn parses_todoist_catalog_spec_into_six_tools() {
        let model = parse_openapi(
            include_bytes!("../../../../../examples/connectors/todoist.openapi.json"),
            None,
            None,
        )
        .expect("todoist catalog spec should parse");

        assert_eq!(model.tools.len(), 6);
        for op in [
            "getTasks",
            "createTask",
            "getTask",
            "closeTask",
            "getProjects",
            "createProject",
        ] {
            assert!(
                model.tools.iter().any(|t| t.name == op),
                "expected a tool named {op}"
            );
        }

        // Path parameter is captured on the by-id operation.
        let get_task = model.tools.iter().find(|t| t.name == "getTask").unwrap();
        assert_eq!(get_task.path_template, "/tasks/{task_id}");
        assert!(
            get_task
                .params
                .iter()
                .any(|p| p.name == "task_id" && matches!(p.location, ParamLoc::Path) && p.required)
        );

        // Request body properties flow into the create-task input schema.
        let create = model.tools.iter().find(|t| t.name == "createTask").unwrap();
        assert!(create.input_schema["properties"]["content"].is_object());
        assert!(create.body.is_some());

        assert!(matches!(model.security, Some(AuthScheme::Bearer { .. })));
        assert_eq!(model.base_url, "https://api.todoist.com/rest/v2");
    }

    #[test]
    fn skips_operation_without_operation_id() {
        // The fixture's `PUT /pets` is intentionally missing `operationId` and
        // must be skipped rather than causing a parse error or an extra tool.
        let model = parse_openapi(include_bytes!("fixtures/petstore-min.json"), None, None)
            .expect("parse should succeed");

        assert_eq!(model.tools.len(), 2);
        assert!(!model.tools.iter().any(|t| t.method == "PUT"));
    }

    #[test]
    fn name_and_base_url_overrides_take_precedence() {
        let model = parse_openapi(
            include_bytes!("fixtures/petstore-min.json"),
            Some("my-petstore"),
            Some("https://override.example.com"),
        )
        .expect("parse should succeed");

        assert_eq!(model.name, "my-petstore");
        assert_eq!(model.base_url, "https://override.example.com");
    }

    #[test]
    fn malformed_spec_returns_err_not_panic() {
        let err = parse_openapi(b"{ not json or yaml : ", None, None);
        assert!(err.is_err());
    }

    const DUP_ID_SPEC: &[u8] = br#"{
        "openapi": "3.0.3",
        "info": { "title": "Dup Id Spec", "version": "1.0.0" },
        "servers": [{ "url": "https://api.example.com" }],
        "paths": {
            "/things/{id}": {
                "get": {
                    "operationId": "getThing",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": { "200": { "description": "OK" } }
                }
            },
            "/things": {
                "get": {
                    "operationId": "getThing",
                    "responses": { "200": { "description": "OK" } }
                }
            }
        }
    }"#;

    #[test]
    fn duplicate_operation_id_is_skipped_with_warning() {
        let model = parse_openapi(DUP_ID_SPEC, None, None).expect("parse should succeed");

        // Only the FIRST occurrence is kept; the duplicate is dropped rather
        // than yielding two tools with the same name.
        assert_eq!(
            model.tools.iter().filter(|t| t.name == "getThing").count(),
            1
        );
        let kept = model.tools.iter().find(|t| t.name == "getThing").unwrap();
        assert_eq!(kept.path_template, "/things/{id}");
    }

    const OAUTH2_THEN_BEARER_SPEC: &[u8] = br#"{
        "openapi": "3.0.3",
        "info": { "title": "Oauth Then Bearer Spec", "version": "1.0.0" },
        "servers": [{ "url": "https://api.example.com" }],
        "paths": {
            "/thing": {
                "get": {
                    "operationId": "getThing",
                    "responses": { "200": { "description": "OK" } }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "oauth2Auth": {
                    "type": "oauth2",
                    "flows": {
                        "clientCredentials": {
                            "tokenUrl": "https://auth.example.com/token",
                            "scopes": {}
                        }
                    }
                },
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer"
                }
            }
        }
    }"#;

    #[test]
    fn security_picks_first_supported_scheme_skipping_oauth2() {
        let model =
            parse_openapi(OAUTH2_THEN_BEARER_SPEC, None, None).expect("parse should succeed");

        assert!(matches!(model.security, Some(AuthScheme::Bearer { .. })));
    }

    const ARRAY_QUERY_SPEC: &[u8] = br#"{
        "openapi": "3.0.3",
        "info": { "title": "Array Query Spec", "version": "1.0.0" },
        "servers": [{ "url": "https://api.example.com" }],
        "paths": {
            "/things": {
                "get": {
                    "operationId": "listThings",
                    "parameters": [
                        {
                            "name": "tags",
                            "in": "query",
                            "required": false,
                            "schema": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        }
                    ],
                    "responses": { "200": { "description": "OK" } }
                }
            }
        }
    }"#;

    #[test]
    fn non_scalar_query_param_warns_but_still_parses() {
        let model = parse_openapi(ARRAY_QUERY_SPEC, None, None).expect("parse should succeed");

        // Degrade, don't crash or skip the whole operation: the tool is still
        // produced and the non-scalar param is kept as-is.
        assert_eq!(model.tools.len(), 1);
        let tool = &model.tools[0];
        assert!(tool.params.iter().any(|p| p.name == "tags"));
    }
}

//! Pure Rust WhatsApp Web Protobuf Extractor for Rose-Baileys.
//!
//! Downloads the live WhatsApp Web service worker, fetches the primary JavaScript bundle,
//! analyzes the AST using `oxc_parser`, and synthesizes a complete, valid `WAProto.proto` schema.

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::SourceType;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct PullSummary {
    pub client_revision: String,
    pub bundle_url: String,
    pub bundle_size_bytes: usize,
    pub total_entities: usize,
    pub root_entities: usize,
    pub schema_bytes: usize,
    pub output_file: Option<String>,
    pub backup_file: Option<String>,
}

#[derive(Debug, Clone)]
struct EnumValue {
    name: String,
    id: i64,
}

#[derive(Debug, Clone)]
struct EnumDef {
    name: String,
    values: Vec<EnumValue>,
}

#[derive(Debug, Clone)]
struct FieldDef {
    name: String,
    id: u32,
    type_str: String,
    flags: Vec<String>,
    packed: bool,
}

#[derive(Debug, Clone)]
struct OneOfDef {
    name: String,
    fields: Vec<FieldDef>,
}

#[derive(Debug, Clone)]
enum MemberDef {
    Field(FieldDef),
    OneOf(OneOfDef),
}

#[derive(Debug, Clone)]
struct MessageDef {
    name: String,
    members: Vec<MemberDef>,
}

#[derive(Debug, Clone)]
enum Entity {
    Message(MessageDef),
    Enum(EnumDef),
}

fn unwrap_paren<'a>(mut expr: &'a Expression<'a>) -> &'a Expression<'a> {
    while let Expression::ParenthesizedExpression(p) = expr {
        expr = &p.expression;
    }
    expr
}

fn unwrap_binary_or<'a>(expr: &'a Expression<'a>, list: &mut Vec<&'a Expression<'a>>) {
    let expr = unwrap_paren(expr);
    if let Expression::BinaryExpression(bin) = expr {
        if bin.operator == BinaryOperator::BitwiseOR {
            unwrap_binary_or(&bin.left, list);
            unwrap_binary_or(&bin.right, list);
            return;
        }
    }
    list.push(expr);
}

fn get_nesting(name: &str) -> &str {
    match name.rfind('$') {
        Some(pos) => &name[..pos],
        None => "",
    }
}

fn unnest_name(name: &str) -> &str {
    match name.rfind('$') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

fn stringify_enum(enum_def: &EnumDef, override_name: Option<&str>, indent: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let name = override_name.unwrap_or(&enum_def.name);
    lines.push(format!("{}enum {} {{", indent, name));
    let inner_indent = format!("{}    ", indent);
    for v in &enum_def.values {
        lines.push(format!("{}{} = {};", inner_indent, v.name, v.id));
    }
    lines.push(format!("{}}}", indent));
    lines
}

fn stringify_field(
    field: &FieldDef,
    parent_name: &str,
    entities: &HashMap<String, Entity>,
    indent: &str,
    is_oneof: bool,
) -> String {
    let mut flags = field.flags.clone();
    if flags.contains(&"required".to_string()) {
        flags = flags
            .into_iter()
            .map(|f| {
                if f == "required" {
                    "optional".to_string()
                } else {
                    f
                }
            })
            .collect();
    }
    if !is_oneof && flags.is_empty() && !field.type_str.starts_with("map<") {
        flags.push("optional".to_string());
    }

    let flag_prefix = if flags.is_empty() || is_oneof {
        String::new()
    } else {
        format!("{} ", flags.join(" "))
    };

    let packed_suffix = if field.packed { " [packed=true]" } else { "" };

    let mut type_display = unnest_name(&field.type_str).to_string();
    if entities.contains_key(&field.type_str) {
        let type_indentation = get_nesting(&field.type_str);
        if !type_indentation.is_empty() && type_indentation != parent_name {
            type_display = format!("{}.{}", type_indentation.replace('$', "."), type_display);
        }
    }

    format!(
        "{}{}{} {} = {}{};",
        indent, flag_prefix, type_display, field.name, field.id, packed_suffix
    )
}

fn stringify_message(
    msg: &MessageDef,
    override_name: Option<&str>,
    entities: &HashMap<String, Entity>,
    nested_children: &HashMap<String, Vec<String>>,
    indent: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    let display_name = override_name.unwrap_or(unnest_name(&msg.name));
    lines.push(format!("{}message {} {{", indent, display_name));
    let inner_indent = format!("{}    ", indent);

    for member in &msg.members {
        match member {
            MemberDef::Field(f) => {
                lines.push(stringify_field(
                    f,
                    &msg.name,
                    entities,
                    &inner_indent,
                    false,
                ));
            }
            MemberDef::OneOf(oneof) => {
                lines.push(format!("{}oneof {} {{", inner_indent, oneof.name));
                let oneof_indent = format!("{}    ", inner_indent);
                for f in &oneof.fields {
                    lines.push(stringify_field(f, &msg.name, entities, &oneof_indent, true));
                }
                lines.push(format!("{}}}", inner_indent));
            }
        }
    }

    if let Some(children) = nested_children.get(&msg.name) {
        let mut sorted_children = children.clone();
        sorted_children.sort();
        for child_name in sorted_children {
            if let Some(child_entity) = entities.get(&child_name) {
                let child_display = if child_name.starts_with(&format!("{}$", msg.name)) {
                    &child_name[msg.name.len() + 1..]
                } else {
                    unnest_name(&child_name)
                };

                match child_entity {
                    Entity::Message(m) => {
                        let child_lines = stringify_message(
                            m,
                            Some(child_display),
                            entities,
                            nested_children,
                            &inner_indent,
                        );
                        lines.extend(child_lines);
                    }
                    Entity::Enum(e) => {
                        let child_lines = stringify_enum(e, Some(child_display), &inner_indent);
                        lines.extend(child_lines);
                    }
                }
            }
        }
    }

    lines.push(format!("{}}}", indent));
    lines
}

/// Parses raw WhatsApp Web JavaScript bundle and returns the generated proto3 specification string,
/// total entity count, and root entity count.
pub fn parse_waproto_from_js(
    js_code: &str,
    wa_version: &str,
) -> Result<(String, usize, usize), String> {
    let patched = js_code.replace("LimitSharing$Trigger", "LimitSharing$TriggerType");

    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &patched, SourceType::default()).parse();

    struct RawModule<'a> {
        #[allow(dead_code)]
        name: &'a str,
        body: &'a FunctionBody<'a>,
    }

    let mut raw_modules = Vec::new();

    for stmt in &ret.program.body {
        if let Statement::ExpressionStatement(expr_stmt) = stmt {
            if let Expression::CallExpression(call) = &expr_stmt.expression {
                if let Expression::Identifier(ident) = &call.callee {
                    if ident.name == "__d" {
                        if let Some(Argument::StringLiteral(mod_name)) = call.arguments.first() {
                            if mod_name.value.ends_with(".pb") || mod_name.value == "WAProtoCompile"
                            {
                                if let Some(arg) = call.arguments.get(2) {
                                    if let Some(expr) = arg.as_expression() {
                                        let unwrapped = unwrap_paren(expr);
                                        if let Expression::FunctionExpression(func) = unwrapped {
                                            if let Some(body) = &func.body {
                                                raw_modules.push(RawModule {
                                                    name: mod_name.value.as_str(),
                                                    body,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut entities: HashMap<String, Entity> = HashMap::new();

    for rmod in &raw_modules {
        let mut var_to_name: HashMap<String, String> = HashMap::new();
        let mut var_to_enum: HashMap<String, Vec<EnumValue>> = HashMap::new();
        let mut var_to_spec: HashMap<String, &ObjectExpression> = HashMap::new();

        // 1. Scan VariableDeclarations for enum values
        for s in &rmod.body.statements {
            if let Statement::VariableDeclaration(vd) = s {
                for decl in &vd.declarations {
                    if let Some(init) = &decl.init {
                        if let Expression::CallExpression(c) = unwrap_paren(init) {
                            if let Some(arg0) = c.arguments.first() {
                                if let Some(arg0_expr) = arg0.as_expression() {
                                    if let Expression::ObjectExpression(obj) =
                                        unwrap_paren(arg0_expr)
                                    {
                                        let mut vals = Vec::new();
                                        for p in &obj.properties {
                                            if let ObjectPropertyKind::ObjectProperty(op) = p {
                                                let key_str = match &op.key {
                                                    PropertyKey::StaticIdentifier(id) => {
                                                        id.name.to_string()
                                                    }
                                                    PropertyKey::StringLiteral(sl) => {
                                                        sl.value.to_string()
                                                    }
                                                    _ => continue,
                                                };
                                                if let Expression::NumericLiteral(num) =
                                                    unwrap_paren(&op.value)
                                                {
                                                    vals.push(EnumValue {
                                                        name: key_str,
                                                        id: num.value as i64,
                                                    });
                                                }
                                            }
                                        }
                                        if !vals.is_empty() {
                                            if let Some(var_name) = decl.id.get_identifier_name() {
                                                var_to_enum.insert(var_name.to_string(), vals);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Scan assignments for names, specs, and exports
        for s in &rmod.body.statements {
            let mut exprs: Vec<&Expression> = Vec::new();
            if let Statement::ExpressionStatement(es) = s {
                if let Expression::SequenceExpression(seq) = &es.expression {
                    for e in &seq.expressions {
                        exprs.push(e);
                    }
                } else {
                    exprs.push(&es.expression);
                }
            }

            for expr in exprs {
                let unwrapped = unwrap_paren(expr);
                if let Expression::AssignmentExpression(assign) = unwrapped {
                    if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left {
                        if let Expression::CallExpression(c) = unwrap_paren(&assign.right) {
                            if let Some(arg0) = c.arguments.first() {
                                if let Some(arg0_expr) = arg0.as_expression() {
                                    if let Expression::ObjectExpression(obj) =
                                        unwrap_paren(arg0_expr)
                                    {
                                        let mut vals = Vec::new();
                                        for p in &obj.properties {
                                            if let ObjectPropertyKind::ObjectProperty(op) = p {
                                                let key_str = match &op.key {
                                                    PropertyKey::StaticIdentifier(id) => {
                                                        id.name.to_string()
                                                    }
                                                    PropertyKey::StringLiteral(sl) => {
                                                        sl.value.to_string()
                                                    }
                                                    _ => continue,
                                                };
                                                if let Expression::NumericLiteral(num) =
                                                    unwrap_paren(&op.value)
                                                {
                                                    vals.push(EnumValue {
                                                        name: key_str,
                                                        id: num.value as i64,
                                                    });
                                                }
                                            }
                                        }
                                        if !vals.is_empty() {
                                            var_to_enum.insert(id.name.to_string(), vals);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let AssignmentTarget::StaticMemberExpression(mem) = &assign.left {
                        if mem.property.name == "name" {
                            if let Expression::Identifier(obj_id) = unwrap_paren(&mem.object) {
                                if let Expression::StringLiteral(name_lit) =
                                    unwrap_paren(&assign.right)
                                {
                                    var_to_name.insert(
                                        obj_id.name.to_string(),
                                        name_lit.value.to_string(),
                                    );
                                }
                            }
                        } else if mem.property.name == "internalSpec" {
                            if let Expression::Identifier(obj_id) = unwrap_paren(&mem.object) {
                                if let Expression::ObjectExpression(obj) =
                                    unwrap_paren(&assign.right)
                                {
                                    var_to_spec.insert(obj_id.name.to_string(), obj);
                                }
                            }
                        } else {
                            if let Expression::Identifier(obj_id) = unwrap_paren(&mem.object) {
                                if obj_id.name == "l" || obj_id.name == "i" {
                                    let export_name = mem.property.name.as_str();
                                    if let Expression::Identifier(val_id) =
                                        unwrap_paren(&assign.right)
                                    {
                                        if let Some(enum_vals) =
                                            var_to_enum.get(val_id.name.as_str())
                                        {
                                            entities.insert(
                                                export_name.to_string(),
                                                Entity::Enum(EnumDef {
                                                    name: export_name.to_string(),
                                                    values: enum_vals.clone(),
                                                }),
                                            );
                                            var_to_name.insert(
                                                val_id.name.to_string(),
                                                export_name.to_string(),
                                            );
                                        }
                                        if let Some(clean_name) = export_name.strip_suffix("Spec") {
                                            var_to_name
                                                .entry(val_id.name.to_string())
                                                .or_insert_with(|| clean_name.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Build message definitions
        for (var_name, spec_obj) in var_to_spec {
            let msg_name = match var_to_name.get(&var_name) {
                Some(n) => n.clone(),
                None => continue,
            };

            let mut members = Vec::new();
            let mut oneofs: HashMap<String, Vec<String>> = HashMap::new();

            for prop in &spec_obj.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = prop {
                    let key_str = match &p.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        PropertyKey::StringLiteral(sl) => sl.value.as_str(),
                        _ => continue,
                    };
                    if key_str == "__oneofs__" {
                        if let Expression::ObjectExpression(oneof_obj) = unwrap_paren(&p.value) {
                            for o_prop in &oneof_obj.properties {
                                if let ObjectPropertyKind::ObjectProperty(op) = o_prop {
                                    let oneof_name = match &op.key {
                                        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                                        PropertyKey::StringLiteral(sl) => sl.value.to_string(),
                                        _ => continue,
                                    };
                                    if let Expression::ArrayExpression(arr) =
                                        unwrap_paren(&op.value)
                                    {
                                        let mut field_names = Vec::new();
                                        for el in &arr.elements {
                                            if let ArrayExpressionElement::StringLiteral(sl) = el {
                                                field_names.push(sl.value.to_string());
                                            }
                                        }
                                        oneofs.insert(oneof_name, field_names);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let mut field_map: HashMap<String, FieldDef> = HashMap::new();
            let mut field_order: Vec<String> = Vec::new();

            for prop in &spec_obj.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = prop {
                    let field_name = match &p.key {
                        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                        PropertyKey::StringLiteral(sl) => sl.value.to_string(),
                        _ => continue,
                    };
                    if field_name.starts_with("__") {
                        continue;
                    }

                    if let Expression::ArrayExpression(arr) = unwrap_paren(&p.value) {
                        let id = match arr.elements.first() {
                            Some(ArrayExpressionElement::NumericLiteral(num)) => num.value as u32,
                            _ => continue,
                        };

                        let mut flags = Vec::new();
                        let mut type_name = String::new();

                        if let Some(type_elem) = arr.elements.get(1) {
                            if let Some(type_expr) = type_elem.as_expression() {
                                let mut parts = Vec::new();
                                unwrap_binary_or(type_expr, &mut parts);
                                for part in parts {
                                    if let Some(mem) = unwrap_paren(part).as_member_expression() {
                                        let prop_name = mem.static_property_name();
                                        if let Some(prop_name) = prop_name {
                                            if let Some(inner_mem) =
                                                unwrap_paren(mem.object()).as_member_expression()
                                            {
                                                if inner_mem.static_property_name() == Some("TYPES")
                                                {
                                                    type_name = prop_name.to_lowercase();
                                                } else if inner_mem.static_property_name()
                                                    == Some("FLAGS")
                                                {
                                                    flags.push(prop_name.to_lowercase());
                                                }
                                            } else if let Expression::AssignmentExpression(_) =
                                                unwrap_paren(mem.object())
                                            {
                                                type_name = prop_name.to_lowercase();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let packed = flags.contains(&"packed".to_string());
                        flags.retain(|f| f != "packed");

                        let resolved_type = if type_name == "message" || type_name == "enum" {
                            if let Some(target_elem) = arr.elements.get(2) {
                                if let Some(target_expr) = target_elem.as_expression() {
                                    let unwrap_target = unwrap_paren(target_expr);
                                    if let Expression::Identifier(id) = unwrap_target {
                                        if let Some(n) = var_to_name.get(id.name.as_str()) {
                                            n.clone()
                                        } else {
                                            id.name.to_string()
                                        }
                                    } else if let Some(mem) = unwrap_target.as_member_expression() {
                                        let prop = mem.static_property_name().unwrap_or("unknown");
                                        if let Some(stripped) = prop.strip_suffix("Spec") {
                                            stripped.to_string()
                                        } else {
                                            prop.to_string()
                                        }
                                    } else {
                                        type_name
                                    }
                                } else {
                                    type_name
                                }
                            } else {
                                type_name
                            }
                        } else if type_name == "map" {
                            if let Some(target_elem) = arr.elements.get(2) {
                                if let Some(target_expr) = target_elem.as_expression() {
                                    if let Expression::ArrayExpression(map_arr) =
                                        unwrap_paren(target_expr)
                                    {
                                        let k = map_arr
                                            .elements
                                            .first()
                                            .and_then(|e| e.as_expression())
                                            .map(|e| {
                                                if let Some(mem) =
                                                    unwrap_paren(e).as_member_expression()
                                                {
                                                    mem.static_property_name()
                                                        .unwrap_or("string")
                                                        .to_lowercase()
                                                } else {
                                                    "string".to_string()
                                                }
                                            })
                                            .unwrap_or_else(|| "string".to_string());

                                        let v = map_arr
                                            .elements
                                            .get(1)
                                            .and_then(|e| e.as_expression())
                                            .map(|e| {
                                                let unwrap_v = unwrap_paren(e);
                                                if let Expression::Identifier(id) = unwrap_v {
                                                    var_to_name
                                                        .get(id.name.as_str())
                                                        .cloned()
                                                        .unwrap_or_else(|| id.name.to_string())
                                                } else if let Some(mem) =
                                                    unwrap_v.as_member_expression()
                                                {
                                                    mem.static_property_name()
                                                        .unwrap_or("string")
                                                        .to_string()
                                                } else {
                                                    "string".to_string()
                                                }
                                            })
                                            .unwrap_or_else(|| "string".to_string());

                                        format!("map<{}, {}>", k, v)
                                    } else {
                                        "map<string, string>".to_string()
                                    }
                                } else {
                                    "map<string, string>".to_string()
                                }
                            } else {
                                "map<string, string>".to_string()
                            }
                        } else {
                            type_name
                        };

                        let field = FieldDef {
                            name: field_name.clone(),
                            id,
                            type_str: resolved_type,
                            flags,
                            packed,
                        };
                        field_map.insert(field_name.clone(), field);
                        field_order.push(field_name);
                    }
                }
            }

            let mut used_in_oneof: BTreeSet<String> = BTreeSet::new();
            for (oneof_name, fields) in &oneofs {
                let mut oneof_fields = Vec::new();
                for fname in fields {
                    if let Some(f) = field_map.get(fname) {
                        oneof_fields.push(f.clone());
                        used_in_oneof.insert(fname.clone());
                    }
                }
                members.push(MemberDef::OneOf(OneOfDef {
                    name: oneof_name.clone(),
                    fields: oneof_fields,
                }));
            }

            for fname in field_order {
                if !used_in_oneof.contains(&fname) {
                    if let Some(f) = field_map.remove(&fname) {
                        members.push(MemberDef::Field(f));
                    }
                }
            }

            entities.insert(
                msg_name.clone(),
                Entity::Message(MessageDef {
                    name: msg_name,
                    members,
                }),
            );
        }
    }

    let total_entities = entities.len();

    // 4. Build hierarchy of parent -> child
    let mut nested_children: HashMap<String, Vec<String>> = HashMap::new();
    let mut root_entity_names: Vec<String> = Vec::new();

    for name in entities.keys() {
        let nesting = get_nesting(name);
        if nesting.is_empty() {
            root_entity_names.push(name.clone());
        } else {
            nested_children
                .entry(nesting.to_string())
                .or_default()
                .push(name.clone());
        }
    }

    root_entity_names.sort();
    let root_entities = root_entity_names.len();

    // 5. Generate proto string
    let mut proto_lines = Vec::new();
    proto_lines.push("syntax = \"proto3\";".to_string());
    proto_lines.push("package proto;".to_string());
    proto_lines.push(String::new());
    proto_lines.push(format!("/// WhatsApp Version: {}", wa_version));
    proto_lines.push(String::new());

    for root_name in &root_entity_names {
        if let Some(entity) = entities.get(root_name) {
            match entity {
                Entity::Message(m) => {
                    let lines = stringify_message(m, None, &entities, &nested_children, "");
                    proto_lines.extend(lines);
                    proto_lines.push(String::new());
                }
                Entity::Enum(e) => {
                    let lines = stringify_enum(e, None, "");
                    proto_lines.extend(lines);
                    proto_lines.push(String::new());
                }
            }
        }
    }

    // Append LID migration helper if not present
    let mut final_proto = proto_lines.join("\n");
    if !final_proto.contains("message LIDMigrationMappingSyncPayload") {
        let lid_helper = "\nmessage LIDMigrationMapping {\n    optional uint64 pn = 1;\n    optional uint64 assignedLid = 2;\n    optional uint64 latestLid = 3;\n    optional uint64 chatDbMigrationTimestamp = 4;\n}\n\nmessage LIDMigrationMappingSyncPayload {\n    repeated LIDMigrationMapping pnToLidMappings = 1;\n    optional uint64 chatDbMigrationTimestamp = 2;\n}\n";
        final_proto.push_str(lid_helper);
    }

    Ok((final_proto, total_entities, root_entities))
}

/// Extracts the bootstrap JavaScript bundle URL from WhatsApp Web's service worker script (`sw.js`).
///
/// Handles both legacy `importScripts("...")` and modern Trusted Types:
/// `importScripts(escapeScriptURLPolicy.createScriptURL(/*BTDS*/"..."))`,
/// as well as direct fallback pattern matching for `static.whatsapp.net` or `web.whatsapp.com` scripts.
pub fn extract_bootstrap_url(sw_text: &str) -> Result<String, String> {
    let clean_sw = sw_text.replace("/*BTDS*/", "");

    // 1. Primary pattern: importScripts(...) with or without createScriptURL
    let script_regex =
        Regex::new(r#"importScripts\((?:[^\)]*createScriptURL\()?\s*["']([^"']+)["']"#)
            .map_err(|e| format!("Regex compilation failed: {}", e))?;

    // 2. Fallback pattern: direct CDN / web script URL
    let fallback_regex = Regex::new(r#"https?:\\?/\\?/(?:static\.whatsapp\.net|web\.whatsapp\.com)\\?/rsrc\.php\\?/[^"'\s)]+\.js"#)
        .map_err(|e| format!("Regex compilation failed: {}", e))?;

    let raw_script_url = script_regex
        .captures(&clean_sw)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .or_else(|| {
            fallback_regex
                .captures(&clean_sw)
                .and_then(|c| c.get(0).map(|m| m.as_str().to_string()))
        })
        .ok_or_else(|| "Failed to locate bootstrap script in sw.js".to_string())?;

    let clean_script_url = raw_script_url.replace('\\', "");
    let bootstrap_url = if clean_script_url.starts_with("http") {
        clean_script_url
    } else {
        format!(
            "https://web.whatsapp.com/{}",
            clean_script_url.trim_start_matches('/')
        )
    };

    Ok(bootstrap_url)
}

/// Extracts WhatsApp Web revision number from `sw.js` content.
///
/// Checks `client_revision`, `__spin_r`, and `app_version` with resilient fallback to "latest".
pub fn extract_client_revision(sw_text: &str) -> String {
    let rev_regex = Regex::new(r#"\\?"(?:client_revision|__spin_r)\\?":\s*(\d+)"#).ok();
    if let Some(re) = rev_regex {
        if let Some(caps) = re.captures(sw_text) {
            if let Some(m) = caps.get(1) {
                return m.as_str().to_string();
            }
        }
    }

    let app_ver_regex = Regex::new(r#"\\?"app_version\\?":\s*\\?"([\d\.]+)"#).ok();
    if let Some(re) = app_ver_regex {
        if let Some(caps) = re.captures(sw_text) {
            if let Some(m) = caps.get(1) {
                return m.as_str().to_string();
            }
        }
    }

    "latest".to_string()
}

/// Pulls the latest WAProto directly from live WhatsApp Web JavaScript chunks over HTTPS,
/// parses AST with `oxc_parser`, and optionally updates the specified destination `.proto` file.
pub async fn pull_latest_waproto(output_path: Option<&Path>) -> Result<PullSummary, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // 1. Fetch service worker
    let sw_url = "https://web.whatsapp.com/sw.js";
    let sw_res = client
        .get(sw_url)
        .header("sec-fetch-site", "none")
        .header("sec-fetch-mode", "no-cors")
        .header("sec-fetch-dest", "script")
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", sw_url, e))?;

    if !sw_res.status().is_success() {
        return Err(format!(
            "Failed to fetch {}: WhatsApp Web returned HTTP {}",
            sw_url,
            sw_res.status()
        ));
    }

    let sw_text = sw_res
        .text()
        .await
        .map_err(|e| format!("Failed to read response from {}: {}", sw_url, e))?;

    // 2. Extract revision
    let version = extract_client_revision(&sw_text);
    let wa_version = format!("2.3000.{}", version);

    // 3. Extract bootstrap JS URL
    let bootstrap_url = extract_bootstrap_url(&sw_text)?;

    // 4. Download primary JS bundle
    let bundle_res = client
        .get(&bootstrap_url)
        .header("sec-fetch-site", "cross-site")
        .header("sec-fetch-mode", "no-cors")
        .header("sec-fetch-dest", "script")
        .header("Referer", "https://web.whatsapp.com/")
        .header("Accept", "*/*")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch JS bundle {}: {}", bootstrap_url, e))?;

    if !bundle_res.status().is_success() {
        return Err(format!(
            "Failed to fetch JS bundle {}: HTTP {}",
            bootstrap_url,
            bundle_res.status()
        ));
    }

    let bundle_text = bundle_res
        .text()
        .await
        .map_err(|e| format!("Failed to read JS bundle: {}", e))?;

    let bundle_size = bundle_text.len();

    // 5. Parse and synthesize protobuf
    let (schema_text, total_entities, root_entities) =
        parse_waproto_from_js(&bundle_text, &wa_version)?;

    let mut output_file = None;
    let mut backup_file = None;

    if let Some(dest) = output_path {
        if dest.exists() {
            let bak = dest.with_extension("proto.bak");
            std::fs::copy(dest, &bak)
                .map_err(|e| format!("Failed to backup existing schema to {:?}: {}", bak, e))?;
            backup_file = Some(bak.to_string_lossy().to_string());
        }

        std::fs::write(dest, &schema_text)
            .map_err(|e| format!("Failed to write schema to {:?}: {}", dest, e))?;
        output_file = Some(dest.to_string_lossy().to_string());
    }

    Ok(PullSummary {
        client_revision: wa_version,
        bundle_url: bootstrap_url,
        bundle_size_bytes: bundle_size,
        total_entities,
        root_entities,
        schema_bytes: schema_text.len(),
        output_file,
        backup_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_waproto_synthetic_js() {
        let js = r#"
        __d("WAWebProtobufsTest.pb", ["$InternalEnum", "WAProtoConst"], (function(t, n, r, o, a, i, l) {
            var e = n("$InternalEnum")({ ACTIVE: 0, INACTIVE: 1 });
            var s = {}, u = {};
            s.name = "TestParent";
            s.internalSpec = {
                id: [1, (e_const = o("WAProtoConst")).TYPES.UINT64],
                status: [2, e_const.TYPES.ENUM, e],
                child: [3, e_const.TYPES.MESSAGE, u]
            };
            u.name = "TestParent$TestChild";
            u.internalSpec = {
                val: [1, e_const.TYPES.STRING]
            };
            l.TestStatus = e;
            l.TestParentSpec = s;
            l.TestChildSpec = u;
        }), 99);
        "#;

        let (proto_str, total, roots) =
            parse_waproto_from_js(js, "2.3000.9999").expect("Failed to parse");
        assert!(proto_str.contains("syntax = \"proto3\";"));
        assert!(proto_str.contains("message TestParent {"));
        assert!(proto_str.contains("message TestChild {"));
        assert!(proto_str.contains("enum TestStatus {"));
        assert!(proto_str.contains("ACTIVE = 0;"));
        assert!(proto_str.contains("optional uint64 id = 1;"));
        assert_eq!(roots, 2); // TestParent and TestStatus are root entities
        assert_eq!(total, 3); // TestParent, TestChild, TestStatus
    }

    #[test]
    fn test_extract_bootstrap_url_trusted_types() {
        let sw = r#"
        if (self.trustedTypes && self.trustedTypes.createPolicy) {
            const escapeScriptURLPolicy = self.trustedTypes.createPolicy("workerPolicy", {
                createScriptURL: url => url
            });
            importScripts(escapeScriptURLPolicy.createScriptURL(/*BTDS*/"https:\/\/static.whatsapp.net\/rsrc.php\/v4\/y0\/r\/9RLkba9gnye.js"));
        } else {
            importScripts(/*BTDS*/"https:\/\/static.whatsapp.net\/rsrc.php\/v4\/y0\/r\/9RLkba9gnye.js");
        }
        "#;
        let url = extract_bootstrap_url(sw).expect("Failed to extract URL");
        assert_eq!(
            url,
            "https://static.whatsapp.net/rsrc.php/v4/y0/r/9RLkba9gnye.js"
        );
    }

    #[test]
    fn test_extract_bootstrap_url_trusted_types_only() {
        let sw = r#"
        const escapeScriptURLPolicy = self.trustedTypes.createPolicy("workerPolicy", {
            createScriptURL: url => url
        });
        importScripts(escapeScriptURLPolicy.createScriptURL("https://static.whatsapp.net/rsrc.php/v4/yK/r/lo_tzPx7tJY.js"));
        "#;
        let url = extract_bootstrap_url(sw).expect("Failed to extract URL");
        assert_eq!(
            url,
            "https://static.whatsapp.net/rsrc.php/v4/yK/r/lo_tzPx7tJY.js"
        );
    }

    #[test]
    fn test_extract_bootstrap_url_legacy() {
        let sw = r#"
        importScripts("https://static.whatsapp.net/rsrc.php/v4/y0/r/legacy.js");
        "#;
        let url = extract_bootstrap_url(sw).expect("Failed to extract URL");
        assert_eq!(
            url,
            "https://static.whatsapp.net/rsrc.php/v4/y0/r/legacy.js"
        );
    }

    #[test]
    fn test_extract_bootstrap_url_fallback_cdn() {
        let sw = r#"
        self.__bootstrap = "https:\/\/static.whatsapp.net\/rsrc.php\/v4\/yZ\/r\/fallback.js";
        "#;
        let url = extract_bootstrap_url(sw).expect("Failed to extract fallback URL");
        assert_eq!(
            url,
            "https://static.whatsapp.net/rsrc.php/v4/yZ/r/fallback.js"
        );
    }

    #[test]
    fn test_extract_client_revision_variants() {
        let sw_escaped = r#"\"spin\":2,\"__spin_r\":1049711438,\"client_revision\":1049711438,"#;
        assert_eq!(extract_client_revision(sw_escaped), "1049711438");

        let sw_spin_r = r#"{"__spin_r":1049888999,"version":"prod"}"#;
        assert_eq!(extract_client_revision(sw_spin_r), "1049888999");

        let sw_app_version = r#"\"app_version\":\"1049.711.438.0 (1049711438)\""#;
        assert_eq!(extract_client_revision(sw_app_version), "1049.711.438.0");
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_pull_latest_waproto() {
        let summary = pull_latest_waproto(None)
            .await
            .expect("Live pull should succeed");
        assert!(summary.bundle_size_bytes > 1_000_000);
        assert!(summary.root_entities > 100);
        assert!(summary.total_entities > 200);
        assert!(summary.client_revision.starts_with("2.3000."));
    }
}

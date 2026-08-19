use mentat_core::{MentatError, RepositoryToolArguments, RepositoryToolCall, RepositoryToolName};
use mentat_inference::{AgentMessageContent, AgentRequest, AgentRole, ToolDefinition};
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

pub(crate) fn request_has_tool_results(request: &AgentRequest) -> bool {
    request
        .messages
        .iter()
        .any(|message| matches!(message.content, AgentMessageContent::ToolResult(_)))
}

pub(crate) fn openai_body(request: &AgentRequest) -> Result<Value, MentatError> {
    let mut messages = vec![json!({
        "role": "system",
        "content": request.effective_system_prompt,
    })];
    for message in &request.messages {
        match (&message.role, &message.content) {
            (AgentRole::User, AgentMessageContent::Text(text)) => {
                messages.push(json!({"role": "user", "content": text}));
            }
            (AgentRole::Assistant, AgentMessageContent::Text(text)) => {
                messages.push(json!({"role": "assistant", "content": text}));
            }
            (AgentRole::Assistant, AgentMessageContent::ToolCalls(calls)) => {
                let calls = calls
                    .iter()
                    .map(|call| {
                        Ok(json!({
                            "id": call.call_id.to_string(),
                            "type": "function",
                            "function": {
                                "name": call.name.wire_name(),
                                "arguments": serde_json::to_string(&arguments_value(&call.arguments))
                                    .map_err(|error| wire_error("AGENT_TOOL_ENCODE_FAILED", &error.to_string()))?,
                            }
                        }))
                    })
                    .collect::<Result<Vec<_>, MentatError>>()?;
                messages.push(json!({"role": "assistant", "tool_calls": calls}));
            }
            (AgentRole::Tool, AgentMessageContent::ToolResult(result)) => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": result.call_id.to_string(),
                    "content": serde_json::to_string(result)
                        .map_err(|error| wire_error("AGENT_TOOL_ENCODE_FAILED", &error.to_string()))?,
                }));
            }
            _ => {
                return Err(wire_error(
                    "AGENT_MESSAGE_ROLE_INVALID",
                    "Agent message role과 content variant가 일치하지 않습니다.",
                ));
            }
        }
    }
    let tools: Vec<_> = request.tools.iter().map(openai_tool).collect();
    let mut body = json!({
        "model": request.profile.model,
        "stream": true,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = Value::String("auto".to_string());
    }
    Ok(body)
}

pub(crate) fn gemini_body(request: &AgentRequest) -> Result<Value, MentatError> {
    let mut contents = Vec::new();
    let mut call_names = std::collections::HashMap::new();
    for message in &request.messages {
        match (&message.role, &message.content) {
            (AgentRole::User, AgentMessageContent::Text(text)) => {
                contents.push(json!({"role": "user", "parts": [{"text": text}]}));
            }
            (AgentRole::Assistant, AgentMessageContent::Text(text)) => {
                contents.push(json!({"role": "model", "parts": [{"text": text}]}));
            }
            (AgentRole::Assistant, AgentMessageContent::ToolCalls(calls)) => {
                let parts: Vec<_> = calls
                    .iter()
                    .map(|call| {
                        call_names.insert(call.call_id, call.name.wire_name().to_string());
                        json!({"functionCall": {
                            "name": call.name.wire_name(),
                            "args": arguments_value(&call.arguments),
                        }})
                    })
                    .collect();
                contents.push(json!({"role": "model", "parts": parts}));
            }
            (AgentRole::Tool, AgentMessageContent::ToolResult(result)) => {
                let name = call_names.get(&result.call_id).ok_or_else(|| {
                    wire_error(
                        "AGENT_TOOL_CALL_MISSING",
                        "Gemini function response에 대응하는 tool call이 없습니다.",
                    )
                })?;
                let response = serde_json::to_value(result)
                    .map_err(|error| wire_error("AGENT_TOOL_ENCODE_FAILED", &error.to_string()))?;
                contents.push(json!({"role": "user", "parts": [{"functionResponse": {
                    "name": name,
                    "response": response,
                }}]}));
            }
            _ => {
                return Err(wire_error(
                    "AGENT_MESSAGE_ROLE_INVALID",
                    "Agent message role과 content variant가 일치하지 않습니다.",
                ));
            }
        }
    }
    let declarations: Vec<_> = request.tools.iter().map(gemini_tool).collect();
    let mut body = json!({
        "system_instruction": {"parts": [{"text": request.effective_system_prompt}]},
        "contents": contents,
        "generationConfig": {"temperature": 0.2, "maxOutputTokens": 4096},
    });
    if !declarations.is_empty() {
        body["tools"] = json!([{"functionDeclarations": declarations}]);
    }
    Ok(body)
}

pub(crate) fn parse_repository_tool_call(
    name: &str,
    args: &Value,
    call_id: Option<&str>,
    snapshot_id: Uuid,
) -> Result<RepositoryToolCall, MentatError> {
    let name = RepositoryToolName::ALL
        .into_iter()
        .find(|candidate| candidate.wire_name() == name)
        .ok_or_else(|| wire_error("AGENT_TOOL_NAME_INVALID", "허용되지 않은 tool 이름입니다."))?;
    let object = args.as_object().ok_or_else(|| {
        wire_error(
            "AGENT_TOOL_SCHEMA_INVALID",
            "tool arguments는 JSON object여야 합니다.",
        )
    })?;
    let arguments = match name {
        RepositoryToolName::RepoStatus => RepositoryToolArguments::RepoStatus,
        RepositoryToolName::ListTree => RepositoryToolArguments::ListTree {
            relative_path: optional_string(object, "relative_path")?.map(PathBuf::from),
            depth: bounded_u64(object, "depth", 1, 4)? as u8,
            limit: bounded_u64(object, "limit", 1, 500)? as u16,
        },
        RepositoryToolName::SearchPaths => RepositoryToolArguments::SearchPaths {
            query: required_string(object, "query")?,
            limit: bounded_u64(object, "limit", 1, 100)? as u16,
        },
        RepositoryToolName::SearchText => RepositoryToolArguments::SearchText {
            query: required_string(object, "query")?,
            path_filter: optional_string(object, "path_filter")?,
            limit: bounded_u64(object, "limit", 1, 100)? as u16,
        },
        RepositoryToolName::ReadFileLines => RepositoryToolArguments::ReadFileLines {
            relative_path: PathBuf::from(required_string(object, "relative_path")?),
            start_line: bounded_u64(object, "start_line", 1, u32::MAX as u64)? as usize,
            end_line: bounded_u64(object, "end_line", 1, u32::MAX as u64)? as usize,
        },
        RepositoryToolName::FileMetadata => RepositoryToolArguments::FileMetadata {
            relative_path: PathBuf::from(required_string(object, "relative_path")?),
        },
    };
    let call_id = call_id
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    Ok(RepositoryToolCall {
        call_id,
        snapshot_id,
        name,
        arguments,
    })
}

fn openai_tool(definition: &ToolDefinition) -> Value {
    json!({"type": "function", "function": {
        "name": definition.name,
        "description": definition.description,
        "parameters": definition.input_schema,
    }})
}

fn gemini_tool(definition: &ToolDefinition) -> Value {
    json!({
        "name": definition.name,
        "description": definition.description,
        "parameters": definition.input_schema,
    })
}

fn arguments_value(arguments: &RepositoryToolArguments) -> Value {
    match arguments {
        RepositoryToolArguments::RepoStatus => json!({}),
        RepositoryToolArguments::ListTree {
            relative_path,
            depth,
            limit,
        } => json!({
            "relative_path": relative_path.as_ref().map(|path| path.to_string_lossy().replace('\\', "/")),
            "depth": depth, "limit": limit,
        }),
        RepositoryToolArguments::SearchPaths { query, limit } => {
            json!({"query": query, "limit": limit})
        }
        RepositoryToolArguments::SearchText {
            query,
            path_filter,
            limit,
        } => {
            json!({"query": query, "path_filter": path_filter, "limit": limit})
        }
        RepositoryToolArguments::ReadFileLines {
            relative_path,
            start_line,
            end_line,
        } => json!({
            "relative_path": relative_path.to_string_lossy().replace('\\', "/"),
            "start_line": start_line, "end_line": end_line,
        }),
        RepositoryToolArguments::FileMetadata { relative_path } => json!({
            "relative_path": relative_path.to_string_lossy().replace('\\', "/"),
        }),
    }
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, MentatError> {
    let value = object.get(key).and_then(Value::as_str).ok_or_else(|| {
        wire_error(
            "AGENT_TOOL_SCHEMA_INVALID",
            "필수 문자열 argument가 없습니다.",
        )
    })?;
    if value.trim().is_empty() || value.len() > 1024 {
        return Err(wire_error(
            "AGENT_TOOL_SCHEMA_INVALID",
            "문자열 argument 길이가 유효하지 않습니다.",
        ));
    }
    Ok(value.to_string())
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, MentatError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.len() <= 1024 => Ok(Some(value.clone())),
        _ => Err(wire_error(
            "AGENT_TOOL_SCHEMA_INVALID",
            "선택 문자열 argument 형식이 유효하지 않습니다.",
        )),
    }
}

fn bounded_u64(
    object: &serde_json::Map<String, Value>,
    key: &str,
    min: u64,
    max: u64,
) -> Result<u64, MentatError> {
    let value = object.get(key).and_then(Value::as_u64).ok_or_else(|| {
        wire_error(
            "AGENT_TOOL_SCHEMA_INVALID",
            "필수 정수 argument가 없습니다.",
        )
    })?;
    if !(min..=max).contains(&value) {
        return Err(wire_error(
            "AGENT_TOOL_SCHEMA_INVALID",
            "정수 argument가 허용 범위를 벗어났습니다.",
        ));
    }
    Ok(value)
}

fn wire_error(code: &str, message: &str) -> MentatError {
    MentatError::BackendError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_write_tool_and_out_of_range_arguments() {
        let snapshot_id = Uuid::new_v4();
        assert!(parse_repository_tool_call("write_file", &json!({}), None, snapshot_id).is_err());
        assert!(parse_repository_tool_call(
            "read_file_lines",
            &json!({"relative_path": "src/lib.rs", "start_line": 1, "end_line": 0}),
            None,
            snapshot_id,
        )
        .is_err());
    }
}

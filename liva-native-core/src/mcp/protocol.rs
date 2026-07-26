use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: String, method: String, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }
}

impl JsonRpcResponse {
    pub fn success(id: String, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: String, code: i32, message: String, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data,
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    /// `#[serde(default)]` cho chiều ĐỌC: trong MCP `description` là trường tuỳ
    /// chọn, và server ngoài bỏ nó thì cả `tools/list` fail deserialize — mất
    /// sạch danh sách tool chỉ vì một tool thiếu mô tả. Chiều GHI
    /// (`mcp/server.rs`) không đổi: 4 tool nội bộ vẫn luôn điền.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: schemars::schema::RootSchema,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolList {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CallToolRequest {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// `#[serde(default)]`: từ MCP 2025-06-18 server có thể chỉ trả
    /// `structuredContent`. Thiếu `content` thì rỗng, không phải hỏng cả lời gọi.
    #[serde(default)]
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    /// `rename_all` ở cấp VARIANT: trên dây MCP trường này là `mimeType`, còn
    /// `#[serde(rename_all)]` ở cấp enum chỉ đổi tên variant nên không với tới
    /// đây. Không có nó thì mọi kết quả tool dạng ảnh đều fail deserialize —
    /// một nhánh chưa bao giờ chạy được vì trước G0 chưa có client nào ĐỌC kiểu
    /// này (`mcp/server.rs` chỉ sinh `Text`, nên chiều ghi không đổi hành vi).
    #[serde(rename = "image", rename_all = "camelCase")]
    Image { data: String, mime_type: String },
    /// Loại nội dung MCP mà client này chưa dùng (`audio`, `resource`,
    /// `resource_link`).
    ///
    /// Không có nhánh bắt-tất này thì một phần tử `content` lạ làm **toàn bộ**
    /// `tools/call` thất bại, kể cả khi phần text ta cần nằm ngay bên cạnh.
    /// Đánh dấu là không hỗ trợ và đi tiếp thì trung thực hơn.
    #[serde(other)]
    Unsupported,
}

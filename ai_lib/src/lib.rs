use std::fmt;
use std::time::Duration;
use derive_builder::Builder;
use enum_as_inner::EnumAsInner;
use reqwest::Client;
use schemars::JsonSchema;
use schemars::gen::SchemaSettings;
use schemars::schema::{RootSchema, Schema, SchemaObject};
use schemars::visit::{Visitor, visit_root_schema, visit_schema, visit_schema_object};
use serde::ser::SerializeMap;
use serde::{Serialize, Deserialize, Serializer};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum Model {
    #[serde(rename = "gpt-3.5-turbo-0613")]
    Gpt3p5Turbo,
    #[serde(rename = "gpt-4-0613")]
    Gpt4,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
    Function,
    System,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<CalledFunction>,
}

impl Message {
    pub fn function_to_content(self) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(serde_json::to_string(&self.function_call.unwrap()).unwrap()),
            function_call: None,
        }
    }

    pub fn user(content: impl fmt::Display) -> Self {
        Self { role: "user".to_string(), content: Some(content.to_string()), function_call: None }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalledFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Function {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Clone, EnumAsInner)]
pub enum FunctionCall {
    Auto,
    Exact {
        name: String,
    }
}

impl Serialize for FunctionCall {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            FunctionCall::Auto => serializer.serialize_str("auto"),
            FunctionCall::Exact { name } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("name", name)?;
                map.end()
            }
        }
    }
}

#[derive(Serialize, Builder)]
#[builder(setter(into))]
pub struct ChatCompletionRequest {
    pub model: Model,
    pub messages: Vec<Message>,
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<Function>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[builder(default)]
    pub temperature: f32,
    #[builder(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub index: i32,
    pub message: Message,
    pub finish_reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

pub struct OpenAIClient {
    client: Client,
    api_key: String,
}

impl OpenAIClient {
    pub fn new() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    pub async fn chat_completion(
        &self,
        req: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, reqwest::Error> {
    
        let mut wait_time = Duration::from_secs(1); // Initial wait time of 1 second
        let max_wait_time = Duration::from_secs(60); // Maximum wait time of 60 seconds
    
        loop {
            let res = self
                .client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(req)
                .send()
                .await?;

            match res.status() {
                reqwest::StatusCode::TOO_MANY_REQUESTS => {
                    if wait_time < max_wait_time {
                        eprint!("Too many requests, waiting {:?}...", wait_time);
                        tokio::time::sleep(wait_time).await;
                        wait_time *= 2; // Double the wait time for the next loop
                    } else {
                        panic!("Exceeded max wait time");
                    }
                }
                _ => {

                    let body = res.text().await.unwrap();

                    return Ok(serde_json::from_str::<ChatCompletionResponse>(&body).unwrap());
                }
            }
        }
    }
}


pub fn schema<T: JsonSchema>() -> serde_json::Value {

    #[derive(Debug, Clone)]    
    struct MyVisitor;
    
    impl Visitor for MyVisitor {
        fn visit_root_schema(&mut self, root: &mut RootSchema) {
            root.meta_schema = None;
            visit_root_schema(self, root)
        }
    
        fn visit_schema(&mut self, schema: &mut Schema) {
            visit_schema(self, schema)
        }
    
        fn visit_schema_object(&mut self, schema: &mut SchemaObject) {
            schema.format = None;
            schema.number = None;
            if let Some(enums) = &schema.enum_values {
                if enums.len() == 1 {
                    schema.const_value = Some(enums[0].clone());
                    schema.enum_values = None;
                }
            }
            if let Some(_obj) = &mut schema.object {
                // obj.required.clear();
            }
            visit_schema_object(self, schema)
        }
    }

    let settings = SchemaSettings::draft2019_09().with(|s| {
        // s.inline_subschemas = true;
        s.visitors.push(Box::new(MyVisitor))
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    let mut value = serde_json::to_value(&schema).unwrap();
    value.as_object_mut().unwrap().remove("title");
    value
}

#[derive(Debug)]
pub enum AiFunctionError {
    Recoverable(String),
    Unrecoverable(String),
}

pub fn done() -> AiFunctionResult {
    Ok(AiFunctionResponse::Done)
}

pub fn recoverable_err(msg: impl ToString) ->  AiFunctionResult {
    Err(AiFunctionError::Recoverable(msg.to_string()))
}

impl From<serde_json::Error> for AiFunctionError {
    fn from(e: serde_json::Error) -> Self {
        Self::Recoverable(e.to_string())
    }
}

pub enum AiFunctionResponse {
    Done,
    Prompt {
        temperature: f32,
        prompt: String,
        functions: Vec<String>,
    }
}

pub type AiFunctionResult = Result<AiFunctionResponse, AiFunctionError>;

pub trait AiInitialState {
    fn initial(&mut self) -> AiFunctionResponse;
}

pub trait AiState : AiInitialState {
    fn json_schema_for_function(function_name: &str) -> Option<Function>;
    fn call_function(&mut self, function_name: &str, arg: &str) -> AiFunctionResult;
}


pub async fn drive<S: AiState>(state: &mut S) -> Result<(), String> {
    let mut next_prompt = state.initial();

    let client = OpenAIClient::new();

    'next: loop {
        match next_prompt {
            AiFunctionResponse::Done => return Ok(()),
            AiFunctionResponse::Prompt { temperature, prompt, functions } => {

                let mut messages = vec![Message::user(prompt)];

                let functions: Vec<_> = functions
                    .into_iter()
                    .map(|f| S::json_schema_for_function(&f).unwrap())
                    .collect();

                let function_call = if functions.len() == 1 {
                    FunctionCall::Exact { name: functions[0].name.clone() }
                } else {
                    FunctionCall::Auto
                };

                for _ in 0..5 {
                    let request = ChatCompletionRequestBuilder::default()
                        .model(Model::Gpt3p5Turbo)
                        .messages(messages.clone())
                        .functions(functions.clone())
                        .function_call(function_call.clone())
                        .temperature(temperature)
                        .build()
                        .unwrap();

                    let response = client.chat_completion(&request).await.unwrap();
                    let message = response.choices[0].message.clone();
                    messages.push(message.clone().function_to_content());
                    match message.function_call {
                        None => {
                            messages.push(Message::user("You must call one of the provided functions"));
                        },
                        Some(CalledFunction { name, arguments }) => {
                            match state.call_function(&name, &arguments) {
                                Ok(next) => {
                                    next_prompt = next;
                                    continue 'next;
                                }
                                Err(AiFunctionError::Recoverable(e)) => {
                                    messages.push(Message::user(format!("Error: {}", e)));
                                },
                                Err(AiFunctionError::Unrecoverable(e)) => {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
                return Err("Too many errors".to_string());
            }
        }
    }
}

pub trait IntoOk<T> {
    fn into_ok(self) -> T;
}

impl IntoOk<AiFunctionResult> for AiFunctionResponse {
    fn into_ok(self) -> AiFunctionResult {
        Ok(self)
    }
}

impl IntoOk<AiFunctionResponse> for AiFunctionResponse {
    fn into_ok(self) -> AiFunctionResponse {
        self
    }
}

#[macro_export]
macro_rules! prompt {
    ($temp:literal, $prompt:literal => [$($fns:ident),*]) => {{
        // Verify that the functions exist
        $(let _ = Self::$fns;)*
        let response = $crate::AiFunctionResponse::Prompt {
            temperature: $temp,
            prompt: format!($prompt),
            functions: vec![$(stringify!($fns).to_string()),*],
        };
        $crate::IntoOk::into_ok(response)
    }};

    ($prompt:literal => [$($fns:ident),*]) => {
        prompt!(0.0, $prompt => [$($fns),*])
    }
}

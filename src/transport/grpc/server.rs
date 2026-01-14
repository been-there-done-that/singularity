//! gRPC Server implementation.

use tonic::{Request, Response, Status, Code};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::transport::AppState;
use crate::transport::pipeline;
use crate::transport::error::TransportError;
use crate::protocol::{OpRequest, OpExecute, Resource, Opcode, CapabilityToken};
use crate::identity::IdentityError;

// Include generated protos
pub mod pb {
    tonic::include_proto!("singularity");
}

use pb::singularity_server::{Singularity, SingularityServer};
use pb::{OpRequestProto, CapGrantProto, OpExecuteProto, ExecutionResultProto, ResourceProto};

pub struct GrpcServer {
    state: AppState,
}

impl GrpcServer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

// Helper: Convert TransportError to Status
impl From<TransportError> for Status {
    fn from(err: TransportError) -> Self {
        match err {
            TransportError::Unauthorized(e) => Status::unauthenticated(e.to_string()),
            TransportError::PolicyDenied => Status::permission_denied("policy denied"),
            TransportError::InvalidCapability(msg) => Status::permission_denied(format!("invalid capability: {}", msg)),
            TransportError::BadRequest(msg) => Status::invalid_argument(msg),
            TransportError::Internal(msg) => Status::internal(msg),
            TransportError::Execution(e) => match e {
                crate::execution::ExecutionError::ResourceNotFound { .. } => Status::not_found(e.to_string()),
                crate::execution::ExecutionError::ConstraintViolation { .. } => Status::failed_precondition(e.to_string()),
                crate::execution::ExecutionError::UnauthorizedFieldWrite { .. } => Status::permission_denied(e.to_string()),
                crate::execution::ExecutionError::OperationNotSupported(_) => Status::invalid_argument(e.to_string()),
                _ => Status::internal(e.to_string()),
            },
            TransportError::State(e) => match e {
                crate::state::StateError::NotFound { .. } => Status::not_found(e.to_string()),
                crate::state::StateError::ConstraintViolation { .. } => Status::failed_precondition(e.to_string()),
                crate::state::StateError::CapabilityNotSupported(_) => Status::unimplemented(e.to_string()),
                crate::state::StateError::BadRequest(msg) => Status::invalid_argument(msg),
                _ => Status::internal(e.to_string()),
            },
        }
    }
}

// Helper: Get current time
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

// Helper: Extract Bearer token (returns String/Owned)
fn extract_token<T>(req: &Request<T>) -> Result<String, Status> {
    let token = req.metadata()
        .get("authorization")
        .ok_or_else(|| Status::unauthenticated("missing authorization token"))?
        .to_str()
        .map_err(|_| Status::unauthenticated("invalid authorization token encoding"))?;
    
    token.strip_prefix("Bearer ")
        .map(|t| t.to_string())
        .ok_or_else(|| Status::unauthenticated("invalid authorization scheme (use Bearer)"))
}

// Data conversion helpers (Manual)
// We cannot rely on prost-types having Serde derived.

fn proto_value_to_json(v: prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match v.kind {
        Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => {
            // Proto numbers are always f64.
            // Try to keep integers if possible? serde_json::Number::from_f64 returns Option.
            serde_json::Number::from_f64(n).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null)
        },
        Some(Kind::StringValue(s)) => serde_json::Value::String(s),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(b),
        Some(Kind::StructValue(s)) => proto_struct_to_json(s),
        Some(Kind::ListValue(l)) => {
             serde_json::Value::Array(l.values.into_iter().map(proto_value_to_json).collect())
        },
    }
}

fn proto_struct_to_json(s: prost_types::Struct) -> serde_json::Value {
    let map = s.fields.into_iter().map(|(k, v)| (k, proto_value_to_json(v))).collect();
    serde_json::Value::Object(map)
}

fn json_to_proto_value(v: serde_json::Value) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match v {
        serde_json::Value::Null => Some(Kind::NullValue(0)), // 0 is NULL_VALUE enum
        serde_json::Value::Bool(b) => Some(Kind::BoolValue(b)),
        serde_json::Value::Number(n) => {
            Some(Kind::NumberValue(n.as_f64().unwrap_or(0.0)))
        },
        serde_json::Value::String(s) => Some(Kind::StringValue(s)),
        serde_json::Value::Array(l) => {
            Some(Kind::ListValue(prost_types::ListValue {
                values: l.into_iter().map(json_to_proto_value).collect(),
            }))
        },
        serde_json::Value::Object(m) => {
            Some(Kind::StructValue(json_to_proto_struct(serde_json::Value::Object(m))))
        },
    };
    prost_types::Value { kind }
}

fn json_to_proto_struct(v: serde_json::Value) -> prost_types::Struct {
    match v {
        serde_json::Value::Object(m) => {
            prost_types::Struct {
                fields: m.into_iter().map(|(k, v)| (k, json_to_proto_value(v))).collect(),
            }
        },
        _ => prost_types::Struct::default(),
    }
}

#[tonic::async_trait]
impl Singularity for GrpcServer {
    async fn request(&self, request: Request<OpRequestProto>) -> Result<Response<CapGrantProto>, Status> {
        let current_time = now();
        let token = extract_token(&request)?; // Returns String (owned)
        let req_proto = request.into_inner();

        // Convert Proto -> Kernel types
        let resource_proto = req_proto.resource.ok_or_else(|| Status::invalid_argument("missing resource"))?;
        let resource = Resource {
            resource_type: resource_proto.r#type,
            resource_id: resource_proto.id,
        };

        // Convert input Struct -> JSON Value
        let input = if let Some(s) = req_proto.input {
            Some(proto_struct_to_json(s))
        } else {
            None
        };

        let op_req = OpRequest::new(
            req_proto.request_id,
            req_proto.op, 
            resource,
            current_time,
        ).with_input(input.unwrap_or(serde_json::Value::Null));
        
        // Pass "token.as_str()" if process_request needs &str, but we have String. 
        // process_request signature: (..., identity_token: &str, ...)
        
        let grant = pipeline::process_request(&self.state, &token, op_req, current_time)
            .map_err(Status::from)?;
            
        Ok(Response::new(CapGrantProto {
            request_id: grant.request_id,
            token: grant.token.into_inner(),
            expires_at: grant.expires_at,
        }))
    }

    async fn execute(&self, request: Request<OpExecuteProto>) -> Result<Response<ExecutionResultProto>, Status> {
        let current_time = now();
        let req_proto = request.into_inner();

        // Convert Proto -> Kernel types
        let payload = if let Some(s) = req_proto.payload {
            Some(proto_struct_to_json(s))
        } else {
            None
        };

        let op_exec = OpExecute::new(
            req_proto.execute_id,
            CapabilityToken::new(req_proto.token),
            current_time,
        );
        let op_exec = if let Some(p) = payload {
            op_exec.with_payload(p)
        } else {
            op_exec
        };

        let result = pipeline::process_execute(&self.state, op_exec, current_time)
            .map_err(Status::from)?;

        // Map Result -> Proto
        let result_proto = match result {
            crate::execution::ExecutionResult::Read { data } => {
                let s = json_to_proto_struct(data); // Infallible manual conversion
                pb::execution_result_proto::Result::Data(s)
            },
            crate::execution::ExecutionResult::Write { affected_count } => {
                pb::execution_result_proto::Result::RowsAffected(affected_count)
            },
            crate::execution::ExecutionResult::NoOp => {
                 pb::execution_result_proto::Result::RowsAffected(0)
            }
        };

        Ok(Response::new(ExecutionResultProto {
            result: Some(result_proto),
        }))
    }
}

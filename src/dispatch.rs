use anyhow::Result;
use core::fmt;
use lsp_server::Message;
use serde::{de::DeserializeOwned, Serialize};

use crate::{error::LspError, state::GlobalState};

/// A visitor for routing a raw JSON request to an appropriate handler function.
///
/// (inspired by rust_analyzer/src/dispatch.rs)
pub struct RequestDispatcher<'a> {
    pub req: Option<lsp_server::Request>,
    pub global_state: &'a mut GlobalState,
}

impl<'a> RequestDispatcher<'a> {
    /// Dispatches the request onto the current thread.
    pub fn on<R>(
        &mut self,
        f: fn(&mut GlobalState, R::Params) -> Result<R::Result>,
    ) -> Result<&mut Self>
    where
        R: lsp_types::request::Request + 'static,
        R::Params: DeserializeOwned + fmt::Debug + 'static,
        R::Result: Serialize + 'static,
    {
        let (id, params) = match self.parse::<R>() {
            Some(it) => it,
            None => return Ok(self),
        };

        let result = f(self.global_state, params);
        let response = result_to_response::<R>(id, result);

        self.global_state.sender.send(Message::Response(response))?;
        Ok(self)
    }

    pub fn finish(&mut self) {
        if let Some(req) = self.req.take() {
            tracing::error!("unknown request: {:?}", req);
            let response = lsp_server::Response::new_err(
                req.id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                "unknown request".to_string(),
            );

            self.global_state
                .sender
                .send(Message::Response(response))
                .unwrap();
        }
    }

    fn parse<R>(&mut self) -> Option<(lsp_server::RequestId, R::Params)>
    where
        R: lsp_types::request::Request + 'static,
        R::Params: DeserializeOwned + fmt::Debug + 'static,
    {
        let req = match &self.req {
            Some(req) if req.method == R::METHOD => self.req.take().unwrap(),
            _ => return None,
        };

        let res = from_json(R::METHOD, req.params);
        match res {
            Ok(params) => Some((req.id, params)),
            Err(err) => {
                let response = lsp_server::Response::new_err(
                    req.id,
                    lsp_server::ErrorCode::InvalidParams as i32,
                    err.to_string(),
                );
                self.global_state
                    .sender
                    .send(Message::Response(response))
                    .unwrap();
                None
            }
        }
    }
}

pub struct NotificationDispatcher<'a> {
    pub notif: Option<lsp_server::Notification>,
    pub global_state: &'a mut GlobalState,
    pub connection: &'a lsp_server::Connection,
}

impl<'a> NotificationDispatcher<'a> {
    pub fn on<N>(&mut self, f: fn(&mut GlobalState, N::Params) -> Result<()>) -> Result<&mut Self>
    where
        N: lsp_types::notification::Notification + 'static,
        N::Params: DeserializeOwned + Send + 'static,
    {
        let notif = match self.notif.take() {
            Some(it) => it,
            None => return Ok(self),
        };
        let params = match notif.extract::<N::Params>(N::METHOD) {
            Ok(it) => it,
            Err(not) => {
                self.notif = Some(not);
                return Ok(self);
            }
        };
        f(self.global_state, params)?;
        Ok(self)
    }

    pub fn finish(&mut self) {
        if let Some(notif) = &self.notif {
            if !notif.method.starts_with("$/") {
                tracing::error!("unhandled notification: {:?}", notif);
            }
        }
    }
}

pub fn from_json<T: DeserializeOwned>(what: &'static str, json: serde_json::Value) -> Result<T> {
    let res = serde_json::from_value(json.clone())
        .map_err(|e| anyhow::anyhow!("Failed to deserialize {}: {}; {}", what, e, json))?;
    Ok(res)
}

fn result_to_response<R>(
    id: lsp_server::RequestId,
    result: Result<R::Result>,
) -> lsp_server::Response
where
    R: lsp_types::request::Request + 'static,
    R::Params: DeserializeOwned + 'static,
    R::Result: Serialize + 'static,
{
    match result {
        Ok(resp) => lsp_server::Response::new_ok(id, &resp),
        Err(e) => match e.downcast::<LspError>() {
            Ok(lsp_error) => lsp_server::Response::new_err(id, lsp_error.code, lsp_error.message),
            Err(e) => lsp_server::Response::new_err(
                id,
                lsp_server::ErrorCode::InternalError as i32,
                e.to_string(),
            ),
        },
    }
}

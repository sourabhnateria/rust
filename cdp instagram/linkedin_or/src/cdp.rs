use anyhow::Result;
use base64::{self, Engine};
use futures::{SinkExt, StreamExt};
use headless_chrome::{Browser, LaunchOptions};
use rand::{Rng, rng, rngs::ThreadRng};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::error::Error;
use tokio::{net::TcpStream, time};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

// const CDPSERVER_URL: &str = "http://127.027.1:9222";

type WS = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Client {
    ws: WS,
    rng: ThreadRng,
    tab_id: Option<String>,
    session_id: Option<String>,
    document_id: Option<u32>,
}

// #[derive(Deserialize, Debug)]
// pub struct Quad {
//     pub x: f64,
//     pub y: f64,
//     pub width: f64,
//     pub height: f64,
// }
#[derive(Deserialize, Debug)]
pub struct BoxModel {
    pub content: [f64; 16],
    pub padding: [f64; 16],
    pub border: [f64; 16],
    pub margin: [f64; 16],
    pub width: f64,
    pub height: f64,
    pub shape_outside: Value,
}

#[derive(Serialize, Deserialize)]
pub struct PdfOptions {
    pub landscape: Option<bool>,
    #[serde(rename = "displayHeaderFooter")]
    pub display_header_footer: Option<bool>,
    #[serde(rename = "printBackground")]
    pub print_background: Option<bool>,
    pub scale: Option<f64>,
    #[serde(rename = "paperWidth")]
    pub paper_width: Option<f64>,
    #[serde(rename = "paperHeight")]
    pub paper_height: Option<f64>,
    #[serde(rename = "marginTop")]
    pub margin_top: Option<f64>,
    #[serde(rename = "marginBottom")]
    pub margin_bottom: Option<f64>,
    #[serde(rename = "marginLeft")]
    pub margin_left: Option<f64>,
    #[serde(rename = "marginRight")]
    pub margin_right: Option<f64>,
    #[serde(rename = "pageRanges")]
    pub page_ranges: Option<String>,
}

impl Client {
    pub async fn new(ws_url: &str) -> Self {
        Self {
            ws: connect_async(ws_url)
                .await
                .expect("Failed to connect to CDP Server")
                .0,
            rng: rng(),
            tab_id: None,
            session_id: None,
            document_id: None,
        }
    }

    async fn get_document_wait(ws: &mut WS, rng: &mut ThreadRng, session_id: &str) -> Result<u32> {
        #[derive(Deserialize)]
        struct PageLoad {
            #[serde(rename = "sessionId")]
            session_id: String,
            method: String,
            params: Value,
        }
        #[derive(Deserialize)]
        struct Res {
            root: Node,
        }
        #[derive(Deserialize)]
        struct Node {
            #[serde(rename = "nodeId")]
            node_id: u32,
        }
        #[derive(Deserialize)]
        struct Response {
            id: Option<i32>,
            error: Option<ErrorMessage>,
            result: Option<Value>,
        }
        #[derive(Deserialize)]
        struct ErrorMessage {
            code: i32,
            message: String,
        }

        let id = rng.random::<i32>();
        ws.send(Message::text(
            json!({"id": id, "sessionId": session_id, "method": "Page.enable"}).to_string(),
        ))
        .await?;

        while let Some(msg) = ws.next().await {
            match msg? {
                Message::Text(text) => match serde_json::from_str(&text) {
                    Ok(PageLoad {
                        session_id: res_session_id,
                        method,
                        params,
                    }) => {
                        if res_session_id != session_id || method != "Page.frameStoppedLoading" {
                            continue;
                        }
                        break;
                    }
                    Err(_) => continue,
                },
                _ => return Err(anyhow::anyhow!("Failed to get response")),
            }
        }

        let id = rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": session_id, "method": "DOM.getDocument"}).to_string();
        ws.send(Message::text(message)).await?;

        while let Some(msg) = ws.next().await {
            match msg? {
                Message::Text(text) => {
                    let response: Response =
                        serde_json::from_str(&text).expect("Failed to parse response and get id");
                    if response.error.is_some() {
                        return Err(anyhow::anyhow!(
                            "Response error: {}",
                            response.error.unwrap().message
                        ));
                    }
                    if response.id != Some(id) {
                        continue;
                    }
                    let res: Res = serde_json::from_value(response.result.unwrap())?;
                    return Ok(res.root.node_id);
                }
                _ => return Err(anyhow::anyhow!("Failed to get response")),
            }
        }
        Err(anyhow::anyhow!("Failed to get response"))
    }

    // In cdp.rs, inside the `impl Client` block

    pub async fn get_inner_text(&mut self, selector: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Res {
            result: ResultValue,
        }
        #[derive(Deserialize)]
        struct ResultValue {
            value: Option<String>,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        // This is the JavaScript code we want to run in the browser
        let expression = format!(r#"document.querySelector('{}').innerText"#, selector);

        let id = self.rng.random::<i32>();
        let message = json!({
            "id": id,
            "sessionId": self.session_id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expression,
                "returnByValue": true // We want the actual string value back
            }
        })
        .to_string();

        let response_value = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response_value)?;

        // The result might be None if the element wasn't found or has no text
        Ok(res.result.value.unwrap_or_default())
    }

    async fn get_document(&mut self) -> Result<u32> {
        #[derive(Deserialize)]
        struct Response {
            id: Option<i32>,
            error: Option<ErrorMessage>,
            result: Option<Value>,
        }
        #[derive(Deserialize)]
        struct ErrorMessage {
            code: i32,
            message: String,
        }
        #[derive(Deserialize)]
        struct Res {
            root: Node,
        }
        #[derive(Deserialize)]
        struct Node {
            #[serde(rename = "nodeId")]
            node_id: u32,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let message = json!({"id": id, "sessionId": self.session_id, "method": "DOM.getDocument"})
            .to_string();
        self.ws.send(Message::text(message)).await?;

        while let Some(msg) = self.ws.next().await {
            match msg? {
                Message::Text(text) => {
                    let response: Response =
                        serde_json::from_str(&text).expect("Failed to parse response and get id");
                    if response.error.is_some() {
                        return Err(anyhow::anyhow!(
                            "Response error: {}",
                            response.error.unwrap().message
                        ));
                    }
                    if response.id != Some(id) {
                        continue;
                    }
                    let res: Res = serde_json::from_value(response.result.unwrap())?;
                    return Ok(res.root.node_id);
                }
                _ => return Err(anyhow::anyhow!("Failed to get response")),
            }
        }
        Err(anyhow::anyhow!("Failed to get response"))
    }

    // =========================================================================
    // THIS IS THE CORRECTED, ROBUST VERSION OF send_get
    // =========================================================================
    async fn send_get(&mut self, id: i32, message: String) -> Result<Value> {
        #[derive(Deserialize)]
        struct Response {
            id: Option<i32>,
            error: Option<ErrorMessage>,
            result: Option<Value>,
        }
        #[derive(Deserialize)]
        struct ErrorMessage {
            code: i32,
            message: String,
        }
        self.ws
            .send(Message::text(message))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {e}"))?;

        // Loop until we get the specific response we are looking for.
        loop {
            if let Some(msg) = self.ws.next().await {
                match msg? {
                    Message::Text(text) => {
                        // Try to parse the message. Some messages might be events we don't care about.
                        if let Ok(response) = serde_json::from_str::<Response>(&text) {
                            // *** THE FIX IS HERE ***
                            // Only process the response if the ID matches the one we sent.
                            if response.id == Some(id) {
                                // If the ID matches, now we can check for an error.
                                if let Some(error) = response.error {
                                    return Err(anyhow::anyhow!(
                                        "Response error for command id {}: {}",
                                        id,
                                        error.message
                                    ));
                                }

                                // If there's no error, we have our result.
                                return Ok(response
                                    .result
                                    .expect("Result field was missing in successful response"));
                            }
                            // If the ID does not match, we do nothing and let the loop continue.
                        }
                        // If the message is not a valid `Response` struct, ignore it and continue.
                    }
                    _ => return Err(anyhow::anyhow!("Received non-text message from websocket")),
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Websocket stream ended before receiving response for id {}",
                    id
                ));
            }
        }
    }

    pub async fn get_tabs(&mut self) -> Result<Vec<HashMap<String, Value>>> {
        #[derive(Deserialize)]
        struct ResultData {
            #[serde(rename = "targetInfos")]
            // The struct now expects a map of String to Value
            target_infos: Vec<HashMap<String, Value>>,
        }

        let id = self.rng.random::<i32>();
        let response = self
            .send_get(
                id,
                json!({"id": id, "method": "Target.getTargets"}).to_string(),
            )
            .await?;
        let res: ResultData = serde_json::from_value(response)?;
        let mut res = res.target_infos;

        // The filter logic now needs to check a `Value`, not a `String`
        res.retain(|v| v.get("type") == Some(&Value::String("page".to_string())));

        Ok(res)
    }

    pub async fn new_tab(&mut self, url: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "targetId")]
            target_id: String,
        }

        let id = self.rng.random::<i32>();
        let response = self
            .send_get(
                id,
                json!({"id": id, "method": "Target.createTarget", "params": {"url": url}})
                    .to_string(),
            )
            .await?;
        let res: Res = serde_json::from_value(response)?;
        self.tab_id = Some(res.target_id.clone());
        Ok(res.target_id)
    }

    pub async fn close_tab(&mut self) -> Result<bool> {
        #[derive(Deserialize)]
        struct Res {
            success: bool,
        }

        if self.tab_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get tab id"));
        }

        let id = self.rng.random::<i32>();
        let response = self
            .send_get(
                id,
                json!({"id": id, "method": "Target.closeTarget", "params": {"targetId": self.tab_id}})
                    .to_string(),
            )
            .await?;
        let res: Res = serde_json::from_value(response)?;
        if res.success {
            self.tab_id = None;
        }
        Ok(res.success)
    }

    pub async fn open_session(
        &mut self,
        tab_id: Option<&str>,
        wait_time: Option<u64>,
    ) -> Result<String> {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "sessionId")]
            session_id: String,
        }
        // Determine which tab_id to use
        let target_tab_id = if tab_id.is_none() {
            self.tab_id.as_ref().expect("Failed to get tab id").clone()
        } else {
            tab_id.unwrap().to_string()
        };

        let id = self.rng.random::<i32>();
        let response = self
        .send_get(
            id,
            json!({"id": id, "method": "Target.attachToTarget", "params": {"targetId": target_tab_id, "flatten": true}}).to_string(),
        )
        .await?;
        let res: Res = serde_json::from_value(response)?;

        // ================== THE FIX IS HERE ==================
        // When we successfully open a session, store both the session ID and the tab ID.
        self.tab_id = Some(target_tab_id); // <-- ADD THIS LINE
        self.session_id = Some(res.session_id.clone());
        // =====================================================

        // println!("Session id: {}", res.session_id);

        if let Some(wait_time) = wait_time {
            time::sleep(time::Duration::from_secs(wait_time)).await;
        }
        let document_id = self.get_document().await?;
        self.document_id = Some(document_id);

        // println!("Document id: {}", document_id);
        Ok(res.session_id)
    }

    pub async fn close_session(&mut self) -> Result<bool> {
        #[derive(Deserialize)]
        struct Res {
            success: bool,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let response = self
            .send_get(
                id,
                json!({
                    "id": id,
                    "method": "Target.detachFromTarget",
                    "params": {"sessionId": self.session_id}
                })
                .to_string(),
            )
            .await?;
        let res: Res = serde_json::from_value(response)?;
        self.session_id = None;
        Ok(res.success)
    }

    pub async fn get_url(&mut self) -> Result<String> {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "targetInfo")]
            target_info: TargetInfo,
        }

        #[derive(Deserialize)]
        struct TargetInfo {
            url: String,
        }

        if self.tab_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get tab id"));
        }
        let id = self.rng.random::<i32>();
        let response = self
            .send_get(
                id,
                json!({"id": id, "method": "Target.getTargetInfo", "params": {"targetId": self.tab_id}}).to_string(),
            )
            .await?;
        let res: Res = serde_json::from_value(response)?;
        Ok(res.target_info.url)
    }

    pub async fn wait_for_tab(&mut self) -> Result<()> {
        #[derive(Deserialize)]
        struct PageLoad {
            #[serde(rename = "sessionId")]
            session_id: String,
            method: String,
            params: Value,
        }
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        while let Some(msg) = self.ws.next().await {
            match msg? {
                Message::Text(text) => match serde_json::from_str(&text) {
                    Ok(PageLoad {
                        session_id: res_session_id,
                        method,
                        params,
                    }) => {
                        if &res_session_id != self.session_id.as_ref().unwrap()
                            || method != "Page.frameStoppedLoading"
                        {
                            continue;
                        }
                        return Ok(());
                    }
                    Err(_) => continue,
                },
                _ => return Err(anyhow::anyhow!("Failed to get response")),
            }
        }
        Err(anyhow::anyhow!("Failed to get response"))
    }

    pub async fn query_selector(&mut self, selector: &str) -> Result<u32> {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "nodeId")]
            node_id: u32,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        if self.document_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get document id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.querySelector", "params": {"nodeId": self.document_id, "selector": selector}}).to_string();
        let response = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response)?;
        Ok(res.node_id)
    }

    pub async fn query_selector_all(&mut self, selector: &str) -> Result<Vec<u32>> {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "nodeIds")]
            node_ids: Vec<u32>,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        if self.document_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get document id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.querySelectorAll", "params": {"nodeId": self.document_id, "selector": selector}}).to_string();
        let response = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response)?;
        Ok(res.node_ids)
    }

    pub async fn get_attributes(&mut self, node_id: u32) -> Result<HashMap<String, String>> {
        #[derive(Deserialize)]
        struct Res {
            attributes: Vec<String>,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.getAttributes", "params": {"nodeId": node_id}}).to_string();
        let response = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response)?;
        let mut attributes = HashMap::new();
        for i in (0..res.attributes.len()).step_by(2) {
            let key = res.attributes[i].clone();
            let value = res.attributes[i + 1].clone();
            attributes.insert(key, value);
        }
        Ok(attributes)
    }

    pub async fn set_attribute(&mut self, node_id: u32, name: &str, value: &str) -> Result<()> {
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.setAttributeValue", "params": {"nodeId": node_id, "name": name, "value": value}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    pub async fn get_html(&mut self, node_id: u32) -> Result<String> {
        #[derive(Deserialize)]
        struct Res {
            #[serde(rename = "outerHTML")]
            outer_html: String,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.getOuterHTML", "params": {"nodeId": node_id}}).to_string();
        let response = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response)?;
        Ok(res.outer_html)
    }

    pub async fn set_html(&mut self, node_id: u32, html: &str) -> Result<()> {
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.setOuterHTML", "params": {"nodeId": node_id, "outerHTML": html}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    pub async fn focus(&mut self, node_id: u32) -> Result<()> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "DOM.focus", "params": {"nodeId": node_id}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    pub async fn get_screenshot(&mut self) -> Result<Vec<u8>> {
        #[derive(Deserialize)]
        struct Res {
            data: String,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "Page.captureScreenshot"})
                .to_string();
        let response = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response)?;

        let engine = base64::engine::general_purpose::STANDARD;
        match engine.decode(&res.data) {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow::anyhow!("Failed to decode base64: {e}")),
        }
    }

    pub async fn get_pdf(&mut self, options: PdfOptions) -> Result<Vec<u8>> {
        #[derive(Deserialize)]
        struct Res {
            data: String,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let id = self.rng.random::<i32>();
        let mut params = serde_json::from_value::<Map<String, Value>>(json!(options))?;
        params.retain(|_, v| !v.is_null());
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "Page.printToPDF", "params": params})
                .to_string();
        let response = self.send_get(id, message).await?;
        let res: Res = serde_json::from_value(response)?;

        let engine = base64::engine::general_purpose::STANDARD;
        match engine.decode(&res.data) {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow::anyhow!("Failed to decode base64: {e}")),
        }
    }

    pub async fn insert_text(&mut self, node_id: u32, text: &str) -> Result<()> {
        self.focus(node_id).await?;

        let id = self.rng.random::<i32>();
        let session_id = self
            .session_id
            .as_ref()
            .expect("Failed to get session id")
            .clone();
        let message =
            json!({"id": id, "sessionId": session_id, "method": "Input.insertText", "params": {"text": text}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    pub async fn press_button(&mut self, node_id: u32, key: &str) -> Result<()> {
        self.focus(node_id).await?;

        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        let message = json!({"id": id, "sessionId": self.session_id, "method": "Input.dispatchKeyEvent", "params": {"type": "keyDown", "key": key}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    // In cdp.rs, find and REPLACE the entire 'click' function with this one:

    pub async fn click(&mut self, node_id: u32, click_count: usize) -> Result<()> {
        #[derive(Debug, Deserialize)]
        struct Quads {
            quads: Vec<[f64; 8]>,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        // Command 1: Get Content Quads (with its own unique ID)
        let id1 = self.rng.random::<i32>();
        let message1 = json!({"id": id1, "sessionId": self.session_id, "method": "DOM.getContentQuads", "params": {"nodeId": node_id}}).to_string();
        let response = self.send_get(id1, message1).await?;
        let res: Quads = serde_json::from_value(response)?;

        // Check if the node is actually visible on the page
        let quads = res.quads.get(0).ok_or_else(|| {
            anyhow::anyhow!("Node has no quads, it might not be visible on the screen")
        })?;
        let mid_x = ((quads[0] + quads[2] + quads[4] + quads[6]) / 4.0).round() as u32;
        let mid_y = ((quads[1] + quads[3] + quads[5] + quads[7]) / 4.0).round() as u32;

        println!("Middle point: ({}, {})", mid_x, mid_y);

        for _ in 0..click_count {
            // Command 2: Mouse Pressed (with its own unique ID)
            let id2 = self.rng.random::<i32>();
            let press = json!({"id": id2,
                    "sessionId": self.session_id,
                    "method": "Input.dispatchMouseEvent",
                    "params": {
                        "type": "mousePressed",
                        "x": mid_x,
                        "y": mid_y,
                        "button": "left",
                        "clickCount": 1
                    }
            })
            .to_string();
            self.send_get(id2, press).await?;

            // Command 3: Mouse Released (with its own unique ID)
            let id3 = self.rng.random::<i32>();
            let release = json!({"id": id3,
                    "sessionId": self.session_id,
                    "method": "Input.dispatchMouseEvent",
                    "params": {
                        "type": "mouseReleased",
                        "x": mid_x,
                        "y": mid_y,
                        "button": "left",
                        "clickCount": 1
                    }
            })
            .to_string();
            self.send_get(id3, release).await?;
        }
        Ok(())
    }

    // In cdp.rs, ADD this new function anywhere inside the `impl Client` block:

    pub async fn scroll_with_mouse_wheel(&mut self, wait_time: Option<u64>) -> Result<()> {
        #[derive(Deserialize, Debug)]
        struct LayoutMetrics {
            #[serde(rename = "visualViewport")]
            visual_viewport: VisualViewport,
        }
        #[derive(Deserialize, Debug)]
        struct VisualViewport {
            #[serde(rename = "clientWidth")]
            client_width: f64,
            #[serde(rename = "clientHeight")]
            client_height: f64,
        }

        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        // Get the size of the visible part of the page to scroll in the middle
        let id_layout = self.rng.random::<i32>();
        let msg_layout = json!({
            "id": id_layout,
            "sessionId": self.session_id,
            "method": "Page.getLayoutMetrics"
        })
        .to_string();
        let layout_metrics_val = self.send_get(id_layout, msg_layout).await?;
        let metrics: LayoutMetrics = serde_json::from_value(layout_metrics_val)?;

        let mid_x = (metrics.visual_viewport.client_width / 2.0).round() as u32;
        let mid_y = (metrics.visual_viewport.client_height / 2.0).round() as u32;

        println!("Scrolling with mouse wheel at ({}, {})", mid_x, mid_y);

        let id = self.rng.random::<i32>();
        let message = json!({
            "id": id,
            "sessionId": self.session_id,
            "method": "Input.dispatchMouseEvent",
            "params": {
                "type": "mouseWheel",
                "x": mid_x,
                "y": mid_y,
                "deltaX": 0,
                "deltaY": 1500 // Scroll down by a large amount
            }
        })
        .to_string();

        self.send_get(id, message).await?;

        if let Some(wait_time) = wait_time {
            time::sleep(time::Duration::from_secs(wait_time)).await;
        }
        Ok(())
    }

    pub async fn scroll_into_view(&mut self, node_id: u32) -> Result<()> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let message = json!({"id": id, "sessionId": self.session_id, "method": "DOM.scrollIntoViewIfNeeded", "params": {"nodeId": node_id}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    pub async fn scroll_to_bottom(&mut self, query: &str, wait_time: Option<u64>) -> Result<()> {
        let nodes = self.query_selector_all(query).await?;
        self.scroll_into_view(nodes[nodes.len() - 1]).await?;
        if let Some(wait_time) = wait_time {
            time::sleep(time::Duration::from_secs(wait_time)).await;
        }
        Ok(())
    }

    pub async fn scroll_page_to_bottom(&mut self, wait_time: Option<u64>) -> Result<()> {
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        // This JavaScript expression tells the browser to scroll to the very bottom of the page
        let expression = "window.scrollTo(0, document.body.scrollHeight)";

        let id = self.rng.random::<i32>();
        let message = json!({
            "id": id,
            "sessionId": self.session_id,
            "method": "Runtime.evaluate",
            "params": { "expression": expression }
        })
        .to_string();

        // We send the command but don't need to process the result
        self.send_get(id, message).await?;

        if let Some(wait_time) = wait_time {
            time::sleep(time::Duration::from_secs(wait_time)).await;
        }

        Ok(())
    }

    pub async fn insert_files(&mut self, node_id: u32, files: Vec<&str>) -> Result<()> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let message = json!({"id": id, "sessionId": self.session_id, "method": "DOM.setFiles", "params": {"nodeId": node_id, "files": files}}).to_string();
        self.send_get(id, message).await?;
        Ok(())
    }

    pub async fn navigate(&mut self, url: &str, wait_time: Option<u64>) -> Result<()> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }

        let message = json!({"id": id, "sessionId": self.session_id, "method": "Page.navigate", "params": {"url": url}}).to_string();
        self.send_get(id, message).await?;

        if let Some(wait_time) = wait_time {
            time::sleep(time::Duration::from_secs(wait_time)).await;
        }
        self.document_id = Some(self.get_document().await?);
        Ok(())
    }

    pub async fn get_node_info(&mut self, node_id: u32) -> Result<Value> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let message = json!({"id": id, "sessionId": self.session_id, "method": "DOM.describeNode", "params": {"nodeId": node_id}}).to_string();
        let response = self.send_get(id, message).await?;
        Ok(response)
    }

    pub async fn get_box_model(&mut self, node_id: u32) -> Result<BoxModel> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let message = json!({"id": id, "sessionId": self.session_id, "method": "DOM.getBoxModel", "params": {"nodeId": node_id}}).to_string();
        let response = self.send_get(id, message).await?;
        let res: BoxModel = serde_json::from_value(response)?;
        Ok(res)
    }

    pub async fn reload(&mut self, wait_time: Option<u64>) -> Result<()> {
        let id = self.rng.random::<i32>();
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("Failed to get session id"));
        }
        let message =
            json!({"id": id, "sessionId": self.session_id, "method": "Page.reload"}).to_string();
        self.send_get(id, message).await?;

        if let Some(wait_time) = wait_time {
            time::sleep(time::Duration::from_secs(wait_time)).await;
        }
        self.document_id = Some(self.get_document().await?);
        Ok(())
    }

    pub async fn refresh_document_id(&mut self) -> Result<()> {
        println!("Refreshing document root...");
        self.document_id = Some(self.get_document().await?);
        Ok(())
    }

    pub async fn press_key(&mut self, key: &str) -> Result<()> {
        if self.session_id.is_none() {
            return Err(anyhow::anyhow!("No active session to press key in."));
        }

        // To properly simulate a key press, we need to send both a "keyDown"
        // and a "keyUp" event. This ensures that any JavaScript listeners
        // for either event are triggered correctly.

        // --- Key Down Event ---
        let id_down = self.rng.random::<i32>();
        let key_down_message = json!({
            "id": id_down,
            "sessionId": self.session_id,
            "method": "Input.dispatchKeyEvent",
            "params": {
                "type": "keyDown",
                "key": key
                // For special keys like "Enter", the 'key' property is sufficient.
                // For character keys, you might also include 'text'.
            }
        })
        .to_string();
        self.send_get(id_down, key_down_message).await?;

        // --- Key Up Event ---
        let id_up = self.rng.random::<i32>();
        let key_up_message = json!({
            "id": id_up,
            "sessionId": self.session_id,
            "method": "Input.dispatchKeyEvent",
            "params": {
                "type": "keyUp",
                "key": key
            }
        })
        .to_string();
        self.send_get(id_up, key_up_message).await?;

        Ok(())
    }
}

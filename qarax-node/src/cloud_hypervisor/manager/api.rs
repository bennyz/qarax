use super::*;

impl VmManager {
    /// Send a raw HTTP/1.1 request over a Unix socket to the Cloud Hypervisor API.
    pub(super) async fn send_api_request(
        socket_path: &PathBuf,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<String, VmManagerError> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(VmManagerError::SpawnError)?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| VmManagerError::ProcessError(e.to_string()))?;

        tokio::spawn(conn);

        let request = if let Some(body_str) = body {
            let body_bytes = Bytes::from(body_str.to_string());
            Request::builder()
                .method(method)
                .uri(format!("http://localhost{}", path))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .body(BoxBody::new(Full::new(body_bytes)))
                .map_err(|e| VmManagerError::ProcessError(e.to_string()))?
        } else {
            Request::builder()
                .method(method)
                .uri(format!("http://localhost{}", path))
                .header("Accept", "application/json")
                .body(BoxBody::new(Empty::new()))
                .map_err(|e| VmManagerError::ProcessError(e.to_string()))?
        };

        let response = sender
            .send_request(request)
            .await
            .map_err(|e| VmManagerError::ProcessError(e.to_string()))?;

        let status = response.status();

        let mut body_bytes = http_body_util::BodyStream::new(response.into_body());
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body_bytes.next().await {
            if let Ok(chunk) = chunk
                && let Ok(data) = chunk.into_data()
            {
                bytes.extend_from_slice(&data);
            }
        }

        let body = String::from_utf8_lossy(&bytes).to_string();

        if !status.is_success() {
            return Err(VmManagerError::ProcessError(format!(
                "API request {} {} failed: HTTP {} — {}",
                method, path, status, body
            )));
        }

        Ok(body)
    }
}

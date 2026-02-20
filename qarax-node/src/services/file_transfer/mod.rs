use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

use crate::rpc::node::{
    CopyFileRequest, DownloadFileRequest, TransferResponse,
    file_transfer_service_server::FileTransferService,
};

/// Implementation of the FileTransferService gRPC service.
///
/// Handles file downloads (HTTP(S) → disk) and local copies on the node.
#[derive(Clone, Default)]
pub struct FileTransferServiceImpl;

impl FileTransferServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

#[tonic::async_trait]
impl FileTransferService for FileTransferServiceImpl {
    async fn download_file(
        &self,
        request: Request<DownloadFileRequest>,
    ) -> Result<Response<TransferResponse>, Status> {
        let req = request.into_inner();
        info!(
            transfer_id = %req.transfer_id,
            source = %req.source_url,
            dest = %req.destination_path,
            "Starting file download"
        );

        match do_download(&req.source_url, &req.destination_path).await {
            Ok(bytes_written) => {
                info!(
                    transfer_id = %req.transfer_id,
                    bytes_written,
                    "Download completed"
                );
                Ok(Response::new(TransferResponse {
                    transfer_id: req.transfer_id,
                    success: true,
                    bytes_written,
                    error: String::new(),
                }))
            }
            Err(e) => {
                error!(
                    transfer_id = %req.transfer_id,
                    error = %e,
                    "Download failed"
                );
                Ok(Response::new(TransferResponse {
                    transfer_id: req.transfer_id,
                    success: false,
                    bytes_written: 0,
                    error: e.to_string(),
                }))
            }
        }
    }

    async fn copy_file(
        &self,
        request: Request<CopyFileRequest>,
    ) -> Result<Response<TransferResponse>, Status> {
        let req = request.into_inner();
        info!(
            transfer_id = %req.transfer_id,
            source = %req.source_path,
            dest = %req.destination_path,
            "Starting local file copy"
        );

        // Ensure parent directory exists
        let dest = std::path::Path::new(&req.destination_path);
        if let Some(parent) = dest.parent()
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            error!(error = %e, "Failed to create destination directory");
            return Ok(Response::new(TransferResponse {
                transfer_id: req.transfer_id,
                success: false,
                bytes_written: 0,
                error: format!("Failed to create directory: {}", e),
            }));
        }

        match tokio::fs::copy(&req.source_path, &req.destination_path).await {
            Ok(bytes_written) => {
                info!(
                    transfer_id = %req.transfer_id,
                    bytes_written,
                    "Local copy completed"
                );
                Ok(Response::new(TransferResponse {
                    transfer_id: req.transfer_id,
                    success: true,
                    bytes_written: bytes_written as i64,
                    error: String::new(),
                }))
            }
            Err(e) => {
                error!(
                    transfer_id = %req.transfer_id,
                    error = %e,
                    "Local copy failed"
                );
                Ok(Response::new(TransferResponse {
                    transfer_id: req.transfer_id,
                    success: false,
                    bytes_written: 0,
                    error: e.to_string(),
                }))
            }
        }
    }
}

/// Download a file from `source_url` to `destination_path`, streaming chunks to disk.
///
/// Writes to a `.tmp` sibling file and atomically renames on success.
/// On failure the partial `.tmp` file is cleaned up.
async fn do_download(source_url: &str, destination_path: &str) -> Result<i64, anyhow::Error> {
    let dest = std::path::Path::new(destination_path);
    let tmp_path = dest.with_extension("tmp");

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Start the HTTP GET and stream chunks to the temp file
    let response = reqwest::get(source_url).await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {status} from {source_url}");
    }

    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut bytes_written: i64 = 0;

    let result: Result<i64, anyhow::Error> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            bytes_written += chunk.len() as i64;
        }
        file.flush().await?;
        Ok(bytes_written)
    }
    .await;

    match result {
        Ok(n) => {
            // Atomic rename into place
            tokio::fs::rename(&tmp_path, dest).await?;
            debug!(bytes = n, "Download written to {}", destination_path);
            Ok(n)
        }
        Err(e) => {
            // Clean up partial temp file
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Err(e)
        }
    }
}

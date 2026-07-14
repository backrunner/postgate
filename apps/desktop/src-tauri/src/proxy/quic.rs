//! Feature-gated HTTP/3 ingress for the proxy.
//!
//! HTTP/3 requests are terminated on a localhost QUIC endpoint and bridged
//! over a pooled loopback HTTP/1 connection into the existing proxy pipeline.
//! Keeping one pipeline is important: rule application, plugins, capture,
//! persistence, upstream pooling, and response streaming stay identical
//! across HTTP/1, HTTP/2, and HTTP/3 ingress.

use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use bytes::{Buf, Bytes};
use h3::server::RequestStream;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use hyper::body::{Body, Frame, Incoming, SizeHint};
use hyper::{Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use quinn::crypto::rustls::QuicServerConfig;
use std::error::Error;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tower_service::Service;

type BoxError = Box<dyn Error + Send + Sync>;
type LoopbackBody = UnsyncBoxBody<Bytes, BoxError>;
type LoopbackClient = Client<LoopbackConnector, LoopbackBody>;

const H3_ALPN: &[u8] = b"h3";
const MAX_H3_FIELD_SECTION_SIZE: u64 = 64 * 1024;

pub struct QuicServer {
    endpoint: quinn::Endpoint,
    accept_task: tokio::task::JoinHandle<()>,
}

impl QuicServer {
    pub fn start(
        listen_addr: SocketAddr,
        loopback_proxy_addr: SocketAddr,
        ca: &CertificateAuthority,
        max_idle_per_host: usize,
        idle_timeout: Duration,
    ) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let certified_key = ca.get_cert_for_host("localhost")?;
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                certified_key.cert_chain.clone(),
                certified_key.key.clone_key(),
            )
            .map_err(|error| {
                PostGateError::Proxy(format!("Failed to build QUIC TLS config: {error}"))
            })?;
        tls.alpn_protocols = vec![H3_ALPN.to_vec()];

        let crypto = QuicServerConfig::try_from(tls).map_err(|error| {
            PostGateError::Proxy(format!("Failed to build QUIC crypto config: {error}"))
        })?;
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
        let mut transport = quinn::TransportConfig::default();
        transport.max_concurrent_bidi_streams(256_u32.into());
        transport.max_concurrent_uni_streams(32_u32.into());
        transport.keep_alive_interval(Some(Duration::from_secs(15)));
        let quic_idle_timeout = idle_timeout
            .try_into()
            .map_err(|error| PostGateError::Proxy(format!("Invalid QUIC idle timeout: {error}")))?;
        transport.max_idle_timeout(Some(quic_idle_timeout));
        server_config.transport_config(Arc::new(transport));

        let endpoint = quinn::Endpoint::server(server_config, listen_addr).map_err(|error| {
            PostGateError::Proxy(format!(
                "Failed to bind QUIC endpoint at {listen_addr}: {error}"
            ))
        })?;
        let local_addr = endpoint.local_addr().map_err(|error| {
            PostGateError::Proxy(format!("Failed to read QUIC local address: {error}"))
        })?;

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(idle_timeout)
            .pool_max_idle_per_host(max_idle_per_host.max(1))
            .build(LoopbackConnector {
                addr: loopback_proxy_addr,
            });

        let accept_endpoint = endpoint.clone();
        let accept_task = tokio::spawn(async move {
            tracing::info!("HTTP/3 proxy ingress listening on udp://{local_addr}");
            while let Some(incoming) = accept_endpoint.accept().await {
                let client = client.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(connection) => {
                            if let Err(error) = handle_connection(connection, client).await {
                                tracing::debug!("HTTP/3 connection ended: {error}");
                            }
                        }
                        Err(error) => tracing::debug!("QUIC handshake failed: {error}"),
                    }
                });
            }
        });

        Ok(Self {
            endpoint,
            accept_task,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.endpoint.local_addr()
    }

    pub async fn shutdown(mut self) {
        self.endpoint.close(0_u32.into(), b"proxy shutdown");
        let _ = tokio::time::timeout(Duration::from_secs(1), self.endpoint.wait_idle()).await;
        self.accept_task.abort();
        let _ = (&mut self.accept_task).await;
    }
}

async fn handle_connection(connection: quinn::Connection, client: LoopbackClient) -> Result<()> {
    let mut h3_connection = h3::server::builder()
        .max_field_section_size(MAX_H3_FIELD_SECTION_SIZE)
        .build(h3_quinn::Connection::new(connection))
        .await
        .map_err(|error| PostGateError::Proxy(format!("HTTP/3 handshake failed: {error}")))?;

    loop {
        let resolver = match h3_connection.accept().await {
            Ok(Some(resolver)) => resolver,
            Ok(None) => return Ok(()),
            Err(error) => {
                return Err(PostGateError::Proxy(format!(
                    "Failed to accept HTTP/3 request: {error}"
                )))
            }
        };
        let client = client.clone();
        tokio::spawn(async move {
            match resolver.resolve_request().await {
                Ok((request, stream)) => {
                    if let Err(error) = bridge_request(request, stream, client).await {
                        tracing::debug!("HTTP/3 request failed: {error}");
                    }
                }
                Err(error) => tracing::debug!("Invalid HTTP/3 request: {error}"),
            }
        });
    }
}

async fn bridge_request(
    request: Request<()>,
    stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    client: LoopbackClient,
) -> std::result::Result<(), BoxError> {
    let (mut send_stream, recv_stream) = stream.split();

    if request.method() == hyper::Method::CONNECT {
        send_error_response(
            &mut send_stream,
            StatusCode::NOT_IMPLEMENTED,
            "HTTP/3 CONNECT and CONNECT-UDP are not supported by this ingress",
        )
        .await?;
        return Ok(());
    }

    if request.uri().scheme().is_none() || request.uri().authority().is_none() {
        send_error_response(
            &mut send_stream,
            StatusCode::BAD_REQUEST,
            "HTTP/3 proxy requests require :scheme and :authority",
        )
        .await?;
        return Ok(());
    }

    let (mut parts, _) = request.into_parts();
    parts.version = hyper::Version::HTTP_11;
    remove_hop_by_hop_headers(&mut parts.headers);
    let request_body = H3IncomingBody::new(recv_stream).boxed_unsync();
    let loopback_request = Request::from_parts(parts, request_body);

    let response = match client.request(loopback_request).await {
        Ok(response) => response,
        Err(error) => {
            send_error_response(
                &mut send_stream,
                StatusCode::BAD_GATEWAY,
                &format!("Local proxy bridge failed: {error}"),
            )
            .await?;
            return Ok(());
        }
    };

    send_loopback_response(send_stream, response).await
}

async fn send_loopback_response(
    mut stream: RequestStream<h3_quinn::SendStream<Bytes>, Bytes>,
    response: Response<Incoming>,
) -> std::result::Result<(), BoxError> {
    let (mut parts, mut body) = response.into_parts();
    remove_hop_by_hop_headers(&mut parts.headers);
    stream
        .send_response(Response::from_parts(parts, ()))
        .await?;

    while let Some(frame) = body.frame().await {
        let frame = frame?;
        match frame.into_data() {
            Ok(data) => stream.send_data(data).await?,
            Err(frame) => {
                if let Ok(mut trailers) = frame.into_trailers() {
                    remove_hop_by_hop_headers(&mut trailers);
                    stream.send_trailers(trailers).await?;
                }
            }
        }
    }
    stream.finish().await?;
    Ok(())
}

async fn send_error_response(
    stream: &mut RequestStream<h3_quinn::SendStream<Bytes>, Bytes>,
    status: StatusCode,
    message: &str,
) -> std::result::Result<(), BoxError> {
    let response = Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .header("content-length", message.len())
        .body(())?;
    stream.send_response(response).await?;
    stream
        .send_data(Bytes::copy_from_slice(message.as_bytes()))
        .await?;
    stream.finish().await?;
    Ok(())
}

fn remove_hop_by_hop_headers(headers: &mut hyper::HeaderMap) {
    for name in [
        "connection",
        "keep-alive",
        "proxy-connection",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

struct H3IncomingBody {
    stream: RequestStream<h3_quinn::RecvStream, Bytes>,
    state: H3BodyState,
}

#[derive(Clone, Copy)]
enum H3BodyState {
    Data,
    Trailers,
    Done,
}

impl H3IncomingBody {
    fn new(stream: RequestStream<h3_quinn::RecvStream, Bytes>) -> Self {
        Self {
            stream,
            state: H3BodyState::Data,
        }
    }
}

impl Body for H3IncomingBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        loop {
            match self.state {
                H3BodyState::Data => match self.stream.poll_recv_data(cx) {
                    Poll::Ready(Ok(Some(mut data))) => {
                        let len = data.remaining();
                        return Poll::Ready(Some(Ok(Frame::data(data.copy_to_bytes(len)))));
                    }
                    Poll::Ready(Ok(None)) => self.state = H3BodyState::Trailers,
                    Poll::Ready(Err(error)) => {
                        self.state = H3BodyState::Done;
                        return Poll::Ready(Some(Err(Box::new(error))));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                H3BodyState::Trailers => match self.stream.poll_recv_trailers(cx) {
                    Poll::Ready(Ok(Some(trailers))) => {
                        self.state = H3BodyState::Done;
                        return Poll::Ready(Some(Ok(Frame::trailers(trailers))));
                    }
                    Poll::Ready(Ok(None)) => self.state = H3BodyState::Done,
                    Poll::Ready(Err(error)) => {
                        self.state = H3BodyState::Done;
                        return Poll::Ready(Some(Err(Box::new(error))));
                    }
                    Poll::Pending => return Poll::Pending,
                },
                H3BodyState::Done => return Poll::Ready(None),
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        matches!(self.state, H3BodyState::Done)
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}

#[derive(Clone)]
struct LoopbackConnector {
    addr: SocketAddr,
}

impl Service<Uri> for LoopbackConnector {
    type Response = TokioIo<ProxyTcpStream>;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = io::Result<Self::Response>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let addr = self.addr;
        Box::pin(async move {
            let stream = TcpStream::connect(addr).await?;
            stream.set_nodelay(true)?;
            Ok(TokioIo::new(ProxyTcpStream(stream)))
        })
    }
}

struct ProxyTcpStream(TcpStream);

impl Connection for ProxyTcpStream {
    fn connected(&self) -> Connected {
        Connected::new().proxy(true)
    }
}

impl AsyncRead for ProxyTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProxyTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::{BodyExt, Full};
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use quinn::crypto::rustls::QuicClientConfig;
    use rustls::pki_types::CertificateDer;
    use std::convert::Infallible;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn http3_request_streams_through_loopback_proxy() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        let proxy_task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let service = service_fn(|request: Request<Incoming>| async move {
                        let uri = request.uri().to_string();
                        let body = request.into_body().collect().await.unwrap().to_bytes();
                        let response = format!("{uri}|{}", String::from_utf8_lossy(&body));
                        Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(response))))
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });

        let ca = CertificateAuthority::new().unwrap();
        let server = QuicServer::start(
            "127.0.0.1:0".parse().unwrap(),
            proxy_addr,
            &ca,
            4,
            Duration::from_secs(10),
        )
        .unwrap();
        let server_addr = server.local_addr().unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(ca.get_ca_der().to_vec()))
            .unwrap();
        let mut tls = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![H3_ALPN.to_vec()];
        let crypto = QuicClientConfig::try_from(tls).unwrap();
        let mut client_config = quinn::ClientConfig::new(Arc::new(crypto));
        client_config.transport_config(Arc::new(quinn::TransportConfig::default()));

        let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(client_config);
        let connection = endpoint
            .connect(server_addr, "localhost")
            .unwrap()
            .await
            .unwrap();
        let (mut driver, mut sender) = h3::client::new(h3_quinn::Connection::new(connection))
            .await
            .unwrap();
        let driver_task = tokio::spawn(async move { driver.wait_idle().await });

        let request = Request::builder()
            .method("POST")
            .uri("http://example.test/path?q=1")
            .body(())
            .unwrap();
        let mut request_stream = sender.send_request(request).await.unwrap();
        request_stream
            .send_data(Bytes::from_static(b"hello-h3"))
            .await
            .unwrap();
        request_stream.finish().await.unwrap();

        let response = request_stream.recv_response().await.unwrap();
        let response_status = response.status();
        let mut response_body = Vec::new();
        while let Some(mut chunk) = request_stream.recv_data().await.unwrap() {
            let len = chunk.remaining();
            response_body.extend_from_slice(&chunk.copy_to_bytes(len));
        }
        assert_eq!(
            response_status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&response_body)
        );
        assert_eq!(response_body, b"http://example.test/path?q=1|hello-h3");

        drop(sender);
        endpoint.close(0_u32.into(), b"test complete");
        driver_task.abort();
        server.shutdown().await;
        proxy_task.abort();
    }
}

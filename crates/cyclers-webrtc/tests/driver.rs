use cyclers::driver::Driver as _;
use cyclers_webrtc::{WebRtcCommand, WebRtcDriver, WebRtcSource};
use futures_lite::stream;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use tokio::test;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<WebRtcCommand>();
    let (_source, _fut): (WebRtcSource<_>, _) = WebRtcDriver.call(sink);
}

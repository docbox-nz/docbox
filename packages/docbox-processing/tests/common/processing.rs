use docbox_processing::{
    ProcessingLayer,
    office::{OfficeConverter, OfficeProcessingLayer, convert_server::OfficeConverterServer},
};
use testcontainers::{
    ContainerAsync, GenericImage,
    core::{IntoContainerPort, WaitFor, wait::HttpWaitStrategy},
    runners::AsyncRunner,
};

/// Create a test container that runs the office-convert-server
///
/// Marked with #[allow(dead_code)] as it is used by tests but
/// rustc doesn't believe us
#[allow(dead_code)]
pub async fn create_processing_layer() -> (ProcessingLayer, ContainerAsync<GenericImage>) {
    let container = GenericImage::new("jacobtread/office-convert-server", "0.2.2")
        .with_exposed_port(3000.tcp())
        .with_wait_for(WaitFor::seconds(5))
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/status").with_expected_status_code(200u16),
        ))
        .start()
        .await
        .unwrap();

    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(3000).await.unwrap();
    let client_url = format!("http://{host}:{host_port}");

    let converter_server =
        OfficeConverterServer::from_addresses([client_url.as_str()], false).unwrap();
    let converter = OfficeConverter::ConverterServer(converter_server);

    let processing = ProcessingLayer {
        office: OfficeProcessingLayer { converter },
    };

    (processing, container)
}

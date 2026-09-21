use dekopon_provider_sdk::ProviderError;
use dekopon_provider_sdk::asset::{self, Encoding, Handle, Info, Writer};

pub(crate) trait Assets {
    type Input;
    type Output;
    fn open(&self, reference: &str) -> Result<Self::Input, ProviderError>;
    fn info(&self, input: &Self::Input) -> Info;
    fn read_all(&self, input: &Self::Input) -> Result<Vec<u8>, ProviderError>;
    fn allocate(
        &self,
        content_type: &str,
        encoding: Encoding,
    ) -> Result<Self::Output, ProviderError>;
    fn write_all(&self, output: &Self::Output, bytes: &[u8]) -> Result<(), ProviderError>;
    fn attach(&self, output: Self::Output) -> Result<Info, ProviderError>;
}

pub(crate) struct Host;
fn failure(e: asset::AssetError) -> ProviderError {
    ProviderError::new(e.code.as_str(), e.message)
}
impl Assets for Host {
    type Input = Handle;
    type Output = Writer;
    fn open(&self, reference: &str) -> Result<Handle, ProviderError> {
        asset::open(reference).map_err(failure)
    }
    fn info(&self, input: &Handle) -> Info {
        input.info()
    }
    fn read_all(&self, input: &Handle) -> Result<Vec<u8>, ProviderError> {
        input.read_all().map_err(failure)
    }
    fn allocate(&self, content_type: &str, encoding: Encoding) -> Result<Writer, ProviderError> {
        asset::allocate(content_type, encoding).map_err(failure)
    }
    fn write_all(&self, output: &Writer, bytes: &[u8]) -> Result<(), ProviderError> {
        output.write_all(bytes).map_err(failure)
    }
    fn attach(&self, output: Writer) -> Result<Info, ProviderError> {
        asset::attach(output).map(|h| h.info()).map_err(failure)
    }
}

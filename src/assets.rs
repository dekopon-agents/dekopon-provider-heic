use crate::{HeicError, error};
use dekopon_provider_sdk::asset::{AssetError, Encoding, Handle, Info, Writer};
use dekopon_provider_sdk::provider::Assets;

pub(crate) trait AssetAccess {
    type Input;
    type Output;
    fn open(&self, reference: &str) -> Result<Self::Input, HeicError>;
    fn info(&self, input: &Self::Input) -> Info;
    fn read_all(&self, input: &Self::Input) -> Result<Vec<u8>, HeicError>;
    fn allocate(&self, content_type: &str, encoding: Encoding) -> Result<Self::Output, HeicError>;
    fn write_all(&self, output: &Self::Output, bytes: &[u8]) -> Result<(), HeicError>;
    fn attach(&self, output: Self::Output) -> Result<Info, HeicError>;
}
pub(crate) struct Host(pub Assets);
fn failure(e: AssetError) -> HeicError {
    // Do not expose host-supplied detail to the shell.
    error(e.code.as_str(), "broker asset operation failed")
}
impl AssetAccess for Host {
    type Input = Handle;
    type Output = Writer;
    fn open(&self, reference: &str) -> Result<Handle, HeicError> {
        self.0.open(reference).map_err(failure)
    }
    fn info(&self, input: &Handle) -> Info {
        input.info()
    }
    fn read_all(&self, input: &Handle) -> Result<Vec<u8>, HeicError> {
        input.read_all().map_err(failure)
    }
    fn allocate(&self, content_type: &str, encoding: Encoding) -> Result<Writer, HeicError> {
        self.0.allocate(content_type, encoding).map_err(failure)
    }
    fn write_all(&self, output: &Writer, bytes: &[u8]) -> Result<(), HeicError> {
        output.write_all(bytes).map_err(failure)
    }
    fn attach(&self, output: Writer) -> Result<Info, HeicError> {
        self.0.attach(output).map(|h| h.info()).map_err(failure)
    }
}

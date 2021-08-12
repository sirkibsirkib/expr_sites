use super::*;

#[derive(Debug)]
pub struct LoggerImpl {
    file: File,
}
impl LoggerImpl {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self { file: File::create(path).unwrap() }
    }
}
impl Logger for LoggerImpl {
    fn line_writer(&mut self) -> Option<&mut dyn Write> {
        write!(&mut self.file, ">> ").unwrap();
        Some(&mut self.file)
    }
}

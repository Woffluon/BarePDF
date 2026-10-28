use barepdf_core::PageCount;
use barepdf_platform::printing::{
    Copies, InstalledPrinter, PrintDuplex, PrintError, PrintJobId, PrintOrientation, PrintPage,
    PrintRange, PrintSelection, PrinterDialog, PrinterSink,
};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

/// Subprocess command execution runner trait for CUPS and Linux print commands.
pub trait CommandRunner: Send + Sync {
    /// Executes the specified program with arguments and returns its output.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if the command cannot be spawned or fails at the OS level.
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, io::Error>;
}

/// The output of a completed subprocess command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status_success: bool,
    pub status_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Standard system command runner using [`std::process::Command`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, io::Error> {
        let output = std::process::Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            status_success: output.status.success(),
            status_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// Checks if CUPS or a system printing spooler is available and responsive.
pub fn is_cups_available(runner: &dyn CommandRunner) -> bool {
    let lpstat_found = runner.run("lpstat", &["-r"]).is_ok();
    let lp_found = runner.run("lp", &["--help"]).is_ok();
    let lpr_found = runner.run("lpr", &["-V"]).is_ok();
    lpstat_found || lp_found || lpr_found
}

/// Enumerates all installed printers on the system via CUPS `lpstat`.
#[must_use]
pub fn enumerate_installed_printers() -> Vec<InstalledPrinter> {
    enumerate_printers_with_runner(&SystemCommandRunner)
}

/// Enumerates installed printers using a specific [`CommandRunner`].
#[must_use]
pub fn enumerate_printers_with_runner(runner: &dyn CommandRunner) -> Vec<InstalledPrinter> {
    let a_output = match runner.run("lpstat", &["-a"]) {
        Ok(output) if output.status_success => String::from_utf8_lossy(&output.stdout).to_string(),
        _ => return Vec::new(),
    };

    let default_dest = match runner.run("lpstat", &["-d"]) {
        Ok(output) if output.status_success => {
            parse_lpstat_default_dest(&String::from_utf8_lossy(&output.stdout))
        }
        _ => None,
    };

    parse_lpstat_printers(&a_output, default_dest.as_deref())
}

/// Parses the default destination string from `lpstat -d` output.
#[must_use]
pub fn parse_lpstat_default_dest(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some((_, dest)) = trimmed.split_once(':') {
            let dest = dest.trim();
            if !dest.is_empty() && !dest.eq_ignore_ascii_case("no system default destination") {
                return Some(dest.to_string());
            }
        }
    }
    None
}

/// Parses printer entries from `lpstat -a` output and flags the default printer if matched.
#[must_use]
pub fn parse_lpstat_printers(output_a: &str, default_dest: Option<&str>) -> Vec<InstalledPrinter> {
    let mut printers = Vec::new();
    for line in output_a.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(name) = trimmed.split_whitespace().next() else {
            continue;
        };
        if name.is_empty() || printers.iter().any(|p: &InstalledPrinter| p.name == name) {
            continue;
        }
        let is_default = default_dest.is_some_and(|d| d == name);
        printers.push(InstalledPrinter {
            name: name.to_string(),
            is_default,
        });
    }
    printers
}

fn adler32(data: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &byte in data {
        s1 = (s1 + u32::from(byte)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    (s2 << 16) | s1
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);

    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    let crc = crc32(&crc_input);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Encodes a tightly packed 32-bit BGRA [`PrintPage`] into a valid standard PNG byte vector.
///
/// # Errors
///
/// Returns [`PrintError::InvalidPage`] if page dimensions or byte length are invalid.
pub fn encode_print_page_to_png(page: PrintPage<'_>) -> Result<Vec<u8>, PrintError> {
    let width = page.width();
    let height = page.height();
    let bgra = page.bgra();

    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(4))
        .ok_or(PrintError::InvalidPage)?;
    let filtered_row_len = row_bytes.checked_add(1).ok_or(PrintError::InvalidPage)?;
    let total_filtered_len = usize::try_from(height)
        .ok()
        .and_then(|h| h.checked_mul(filtered_row_len))
        .ok_or(PrintError::InvalidPage)?;

    let mut raw_filtered = Vec::with_capacity(total_filtered_len);
    for row in 0..height as usize {
        raw_filtered.push(0_u8);
        let row_start = row * row_bytes;
        let row_end = row_start + row_bytes;
        let row_bgra = &bgra[row_start..row_end];
        for pixel in row_bgra.chunks_exact(4) {
            raw_filtered.push(pixel[2]);
            raw_filtered.push(pixel[1]);
            raw_filtered.push(pixel[0]);
            raw_filtered.push(pixel[3]);
        }
    }

    let adler = adler32(&raw_filtered);

    let mut zlib_data = Vec::new();
    zlib_data.push(0x78);
    zlib_data.push(0x01);

    const MAX_DEFLATE_BLOCK: usize = 65535;
    let mut offset = 0;
    while offset < raw_filtered.len() {
        let remaining = raw_filtered.len() - offset;
        let chunk_len = remaining.min(MAX_DEFLATE_BLOCK);
        let is_last = offset + chunk_len == raw_filtered.len();

        let header_byte = if is_last { 0x01 } else { 0x00 };
        zlib_data.push(header_byte);

        #[allow(clippy::cast_possible_truncation)]
        let len_u16 = chunk_len as u16;
        let nlen_u16 = !len_u16;
        zlib_data.extend_from_slice(&len_u16.to_le_bytes());
        zlib_data.extend_from_slice(&nlen_u16.to_le_bytes());

        zlib_data.extend_from_slice(&raw_filtered[offset..offset + chunk_len]);
        offset += chunk_len;
    }

    zlib_data.extend_from_slice(&adler.to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr_data = Vec::with_capacity(13);
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(8);
    ihdr_data.push(6);
    ihdr_data.push(0);
    ihdr_data.push(0);
    ihdr_data.push(0);

    write_png_chunk(&mut png, b"IHDR", &ihdr_data);
    write_png_chunk(&mut png, b"IDAT", &zlib_data);
    write_png_chunk(&mut png, b"IEND", &[]);

    Ok(png)
}

/// Linux printer spooling sink that writes rendered pages as PNG files and spools them via CUPS `lp`/`lpr`.
pub struct LinuxPrinterSink {
    job_id: PrintJobId,
    target_dpi: u16,
    printer_name: String,
    orientation: PrintOrientation,
    duplex: PrintDuplex,
    copies: u16,
    title: Option<String>,
    temp_dir: Option<tempfile::TempDir>,
    page_files: Vec<PathBuf>,
    started: bool,
    runner: Option<Arc<dyn CommandRunner>>,
}

impl LinuxPrinterSink {
    pub const DEFAULT_DPI: u16 = 300;
    pub const MAX_DPI: u16 = 600;
    const MIN_DPI: u16 = 1;

    /// Creates a printer sink targeting a specific printer directly, without showing a GUI dialog.
    ///
    /// # Errors
    ///
    /// Returns [`PrintError::InvalidDpi`] if DPI is invalid, [`PrintError::InvalidCopies`] if copies is zero,
    /// [`PrintError::Unavailable`] if CUPS is not present, or [`PrintError::Platform`] if the printer is unknown.
    pub fn direct(
        job_id: PrintJobId,
        target_dpi: u16,
        printer_name: &str,
        orientation: PrintOrientation,
        duplex: PrintDuplex,
        copies: u16,
    ) -> Result<Self, PrintError> {
        Self::direct_with_runner(
            job_id,
            target_dpi,
            printer_name,
            orientation,
            duplex,
            copies,
            None,
        )
    }

    /// Creates a printer sink with a custom [`CommandRunner`] for testing and mocking.
    ///
    /// # Errors
    ///
    /// Returns an error if settings are invalid or CUPS is unavailable.
    pub fn direct_with_runner(
        job_id: PrintJobId,
        target_dpi: u16,
        printer_name: &str,
        orientation: PrintOrientation,
        duplex: PrintDuplex,
        copies: u16,
        runner: Option<Arc<dyn CommandRunner>>,
    ) -> Result<Self, PrintError> {
        if !(Self::MIN_DPI..=Self::MAX_DPI).contains(&target_dpi) {
            return Err(PrintError::InvalidDpi(target_dpi));
        }
        if copies == 0 {
            return Err(PrintError::InvalidCopies(copies));
        }

        let cmd_runner: &dyn CommandRunner = runner.as_deref().unwrap_or(&SystemCommandRunner);
        if !is_cups_available(cmd_runner) {
            return Err(PrintError::Unavailable);
        }

        if !printer_name.is_empty() {
            let installed = enumerate_printers_with_runner(cmd_runner);
            if !installed.iter().any(|p| p.name == printer_name) {
                return Err(PrintError::Platform {
                    operation: "printer not found",
                    code: 1,
                });
            }
        }

        Ok(Self {
            job_id,
            target_dpi,
            printer_name: printer_name.to_string(),
            orientation,
            duplex,
            copies,
            title: None,
            temp_dir: None,
            page_files: Vec::new(),
            started: false,
            runner,
        })
    }
}

impl PrinterSink for LinuxPrinterSink {
    fn job_id(&self) -> PrintJobId {
        self.job_id
    }

    fn target_dpi(&self) -> u16 {
        self.target_dpi
    }

    fn begin(&mut self, title: &str) -> Result<(), PrintError> {
        if self.started {
            return Err(PrintError::InvalidState);
        }
        let cmd_runner: &dyn CommandRunner = self.runner.as_deref().unwrap_or(&SystemCommandRunner);
        if !is_cups_available(cmd_runner) {
            return Err(PrintError::Unavailable);
        }

        let temp_dir = tempfile::Builder::new()
            .prefix("barepdf_print_")
            .tempdir()
            .map_err(|_| PrintError::Platform {
                operation: "create temp directory",
                code: 0,
            })?;

        self.temp_dir = Some(temp_dir);
        self.title = Some(title.to_string());
        self.page_files.clear();
        self.started = true;
        Ok(())
    }

    fn write_page(&mut self, page: PrintPage<'_>) -> Result<(), PrintError> {
        if !self.started {
            return Err(PrintError::InvalidState);
        }
        let temp_dir = self.temp_dir.as_ref().ok_or(PrintError::InvalidState)?;

        let png_bytes = encode_print_page_to_png(page)?;
        let page_filename = format!("page_{:04}.png", self.page_files.len());
        let page_path = temp_dir.path().join(page_filename);
        std::fs::write(&page_path, &png_bytes).map_err(|_| PrintError::Platform {
            operation: "write page file",
            code: 0,
        })?;

        self.page_files.push(page_path);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), PrintError> {
        if !self.started || self.page_files.is_empty() {
            return Err(PrintError::InvalidState);
        }

        let cmd_runner: &dyn CommandRunner = self.runner.as_deref().unwrap_or(&SystemCommandRunner);

        let mut lp_args: Vec<String> = Vec::new();
        if !self.printer_name.is_empty() {
            lp_args.push("-d".to_string());
            lp_args.push(self.printer_name.clone());
        }
        if self.copies > 1 {
            lp_args.push("-n".to_string());
            lp_args.push(self.copies.to_string());
        }
        if let Some(ref title) = self.title {
            lp_args.push("-t".to_string());
            lp_args.push(title.clone());
        }
        match self.orientation {
            PrintOrientation::Portrait => {
                lp_args.push("-o".to_string());
                lp_args.push("orientation-requested=3".to_string());
            }
            PrintOrientation::Landscape => {
                lp_args.push("-o".to_string());
                lp_args.push("orientation-requested=4".to_string());
            }
            PrintOrientation::Auto => {}
        }
        match self.duplex {
            PrintDuplex::TwoSidedLongEdge => {
                lp_args.push("-o".to_string());
                lp_args.push("sides=two-sided-long-edge".to_string());
            }
            PrintDuplex::TwoSidedShortEdge => {
                lp_args.push("-o".to_string());
                lp_args.push("sides=two-sided-short-edge".to_string());
            }
            PrintDuplex::OneSided => {
                lp_args.push("-o".to_string());
                lp_args.push("sides=one-sided".to_string());
            }
        }
        for path in &self.page_files {
            lp_args.push(path.to_string_lossy().to_string());
        }

        let lp_args_refs: Vec<&str> = lp_args.iter().map(String::as_str).collect();
        match cmd_runner.run("lp", &lp_args_refs) {
            Ok(output) if output.status_success => {
                tracing::info!(job_id = self.job_id.get(), "Spool job finished via lp");
                return Ok(());
            }
            Ok(output) => {
                let code = output.status_code.unwrap_or(1) as u32;
                return Err(PrintError::Platform {
                    operation: "lp print command failed",
                    code,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                tracing::debug!("lp command not found, falling back to lpr");
            }
            Err(e) => {
                return Err(PrintError::Platform {
                    operation: "lp execution error",
                    code: e.raw_os_error().unwrap_or(1) as u32,
                });
            }
        }

        let mut lpr_args: Vec<String> = Vec::new();
        if !self.printer_name.is_empty() {
            lpr_args.push(format!("-P{}", self.printer_name));
        }
        if self.copies > 1 {
            lpr_args.push(format!("-#{}", self.copies));
        }
        if let Some(ref title) = self.title {
            lpr_args.push("-T".to_string());
            lpr_args.push(title.clone());
        }
        match self.orientation {
            PrintOrientation::Portrait => {
                lpr_args.push("-o".to_string());
                lpr_args.push("orientation-requested=3".to_string());
            }
            PrintOrientation::Landscape => {
                lpr_args.push("-o".to_string());
                lpr_args.push("orientation-requested=4".to_string());
            }
            PrintOrientation::Auto => {}
        }
        match self.duplex {
            PrintDuplex::TwoSidedLongEdge => {
                lpr_args.push("-o".to_string());
                lpr_args.push("sides=two-sided-long-edge".to_string());
            }
            PrintDuplex::TwoSidedShortEdge => {
                lpr_args.push("-o".to_string());
                lpr_args.push("sides=two-sided-short-edge".to_string());
            }
            PrintDuplex::OneSided => {
                lpr_args.push("-o".to_string());
                lpr_args.push("sides=one-sided".to_string());
            }
        }
        for path in &self.page_files {
            lpr_args.push(path.to_string_lossy().to_string());
        }

        let lpr_args_refs: Vec<&str> = lpr_args.iter().map(String::as_str).collect();
        match cmd_runner.run("lpr", &lpr_args_refs) {
            Ok(output) if output.status_success => {
                tracing::info!(job_id = self.job_id.get(), "Spool job finished via lpr");
                Ok(())
            }
            Ok(output) => {
                let code = output.status_code.unwrap_or(1) as u32;
                Err(PrintError::Platform {
                    operation: "lpr print command failed",
                    code,
                })
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(PrintError::Unavailable),
            Err(e) => Err(PrintError::Platform {
                operation: "lpr execution error",
                code: e.raw_os_error().unwrap_or(1) as u32,
            }),
        }
    }
}

/// Linux printer dialog implementation using CUPS printer enumeration and direct sink creation.
pub struct LinuxPrinterDialog {
    target_dpi: u16,
    runner: Option<Arc<dyn CommandRunner>>,
}

impl LinuxPrinterDialog {
    pub const DEFAULT_DPI: u16 = 300;
    pub const MAX_DPI: u16 = 600;
    const MIN_DPI: u16 = 1;

    #[must_use]
    pub fn new() -> Self {
        Self {
            target_dpi: Self::DEFAULT_DPI,
            runner: None,
        }
    }

    /// Creates a dialog with a custom runner for mocking.
    #[must_use]
    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            target_dpi: Self::DEFAULT_DPI,
            runner: Some(runner),
        }
    }

    /// Sets the target raster DPI for the dialog.
    ///
    /// # Errors
    ///
    /// Returns [`PrintError::InvalidDpi`] if DPI is outside `1..=600`.
    pub fn with_target_dpi(mut self, dpi: u16) -> Result<Self, PrintError> {
        if !(Self::MIN_DPI..=Self::MAX_DPI).contains(&dpi) {
            return Err(PrintError::InvalidDpi(dpi));
        }
        self.target_dpi = dpi;
        Ok(self)
    }

    /// Shows the printer chooser selection with given default parameters.
    ///
    /// # Errors
    ///
    /// Returns [`PrintError::Unavailable`] if CUPS is not installed, or other [`PrintError`] on failure.
    pub fn select_with_defaults(
        &mut self,
        job_id: PrintJobId,
        _page_count: PageCount,
        range: PrintRange,
        orientation_index: i32,
    ) -> Result<Option<PrintSelection<LinuxPrinterSink>>, PrintError> {
        let runner_ref: &dyn CommandRunner = self.runner.as_deref().unwrap_or(&SystemCommandRunner);

        if !is_cups_available(runner_ref) {
            return Err(PrintError::Unavailable);
        }

        let printers = enumerate_printers_with_runner(runner_ref);
        if printers.is_empty() {
            return Ok(None);
        }

        let selected = printers
            .iter()
            .find(|p| p.is_default)
            .or_else(|| printers.first())
            .expect("printers list is not empty");

        let orientation = PrintOrientation::from_index(orientation_index);
        let sink = LinuxPrinterSink::direct_with_runner(
            job_id,
            self.target_dpi,
            &selected.name,
            orientation,
            PrintDuplex::OneSided,
            1,
            self.runner.clone(),
        )?;

        Ok(Some(PrintSelection {
            sink,
            range,
            copies: Copies::default(),
        }))
    }
}

impl Default for LinuxPrinterDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl PrinterDialog for LinuxPrinterDialog {
    type Sink = LinuxPrinterSink;

    fn select(
        &mut self,
        job_id: PrintJobId,
        page_count: PageCount,
    ) -> Result<Option<PrintSelection<Self::Sink>>, PrintError> {
        self.select_with_defaults(job_id, page_count, PrintRange::all(page_count), 0)
    }
}

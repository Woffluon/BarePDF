use barepdf_core::PageCount;
use barepdf_platform::printing::{
    Copies, PrintDuplex, PrintError, PrintJobId, PrintOrientation, PrintPage, PrintRange,
    PrinterDialog, PrinterSink,
};
use barepdf_platform_linux::printing::{
    encode_print_page_to_png, enumerate_printers_with_runner, is_cups_available,
    parse_lpstat_default_dest, parse_lpstat_printers, CommandOutput, CommandRunner,
    LinuxPrinterDialog, LinuxPrinterSink,
};
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MockCommandRunner {
    calls: Mutex<Vec<(String, Vec<String>)>>,
    lpstat_r_result: Option<Result<CommandOutput, io::ErrorKind>>,
    lpstat_a_result: Option<Result<CommandOutput, io::ErrorKind>>,
    lpstat_d_result: Option<Result<CommandOutput, io::ErrorKind>>,
    lp_result: Option<Result<CommandOutput, io::ErrorKind>>,
    lpr_result: Option<Result<CommandOutput, io::ErrorKind>>,
}

impl MockCommandRunner {
    fn new() -> Self {
        Self::default()
    }

    fn record_call(&self, program: &str, args: &[&str]) {
        self.calls.lock().unwrap().push((
            program.to_string(),
            args.iter().map(|s| (*s).to_string()).collect(),
        ));
    }

    fn recorded_calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for MockCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, io::Error> {
        self.record_call(program, args);
        match program {
            "lpstat" => {
                if args.contains(&"-r") {
                    if let Some(ref res) = self.lpstat_r_result {
                        return res.clone().map_err(io::Error::from);
                    }
                }
                if args.contains(&"-a") {
                    if let Some(ref res) = self.lpstat_a_result {
                        return res.clone().map_err(io::Error::from);
                    }
                }
                if args.contains(&"-d") {
                    if let Some(ref res) = self.lpstat_d_result {
                        return res.clone().map_err(io::Error::from);
                    }
                }
                Ok(CommandOutput {
                    status_success: true,
                    status_code: Some(0),
                    stdout: b"scheduler is running\n".to_vec(),
                    stderr: Vec::new(),
                })
            }
            "lp" => {
                if let Some(ref res) = self.lp_result {
                    return res.clone().map_err(io::Error::from);
                }
                Ok(CommandOutput {
                    status_success: true,
                    status_code: Some(0),
                    stdout: b"request id is HP-123 (1 file(s))\n".to_vec(),
                    stderr: Vec::new(),
                })
            }
            "lpr" => {
                if let Some(ref res) = self.lpr_result {
                    return res.clone().map_err(io::Error::from);
                }
                Ok(CommandOutput {
                    status_success: true,
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            _ => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("command not found: {program}"),
            )),
        }
    }
}

#[test]
fn test_parse_lpstat_default_dest() {
    let output = "system default destination: HP_LaserJet_Pro\n";
    assert_eq!(
        parse_lpstat_default_dest(output),
        Some("HP_LaserJet_Pro".to_string())
    );

    let output_none = "no system default destination\n";
    assert_eq!(parse_lpstat_default_dest(output_none), None);

    let output_empty = "";
    assert_eq!(parse_lpstat_default_dest(output_empty), None);
}

#[test]
fn test_parse_lpstat_printers_multiple_and_default() {
    let output_a = r#"
HP_LaserJet_Pro accepting requests since Mon 26 Oct 2026 10:00:00 AM
PDF_Virtual accepting requests since Mon 26 Oct 2026 10:00:00 AM
Office_Deskjet not accepting requests since Sun 25 Oct 2026 08:00:00 PM
"#;
    let printers = parse_lpstat_printers(output_a, Some("PDF_Virtual"));
    assert_eq!(printers.len(), 3);
    assert_eq!(printers[0].name, "HP_LaserJet_Pro");
    assert!(!printers[0].is_default);

    assert_eq!(printers[1].name, "PDF_Virtual");
    assert!(printers[1].is_default);

    assert_eq!(printers[2].name, "Office_Deskjet");
    assert!(!printers[2].is_default);
}

#[test]
fn test_cups_availability_detection() {
    let runner_available = MockCommandRunner::new();
    assert!(is_cups_available(&runner_available));

    let mut runner_unavailable = MockCommandRunner::new();
    runner_unavailable.lpstat_r_result = Some(Err(io::ErrorKind::NotFound));
    runner_unavailable.lp_result = Some(Err(io::ErrorKind::NotFound));
    runner_unavailable.lpr_result = Some(Err(io::ErrorKind::NotFound));
    assert!(!is_cups_available(&runner_unavailable));
}

#[test]
fn test_enumerate_printers_with_mock_runner() {
    let mut runner = MockCommandRunner::new();
    runner.lpstat_a_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"Printer1 accepting requests\nPrinter2 accepting requests\n".to_vec(),
        stderr: Vec::new(),
    }));
    runner.lpstat_d_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"system default destination: Printer2\n".to_vec(),
        stderr: Vec::new(),
    }));

    let printers = enumerate_printers_with_runner(&runner);
    assert_eq!(printers.len(), 2);
    assert_eq!(printers[0].name, "Printer1");
    assert!(!printers[0].is_default);
    assert_eq!(printers[1].name, "Printer2");
    assert!(printers[1].is_default);
}

#[test]
fn test_png_encoding_validity() {
    let width = 2_u32;
    let height = 2_u32;
    // 2x2 pixels BGRA: blue, green, red, white
    let bgra = [
        255, 0, 0, 255, // Blue
        0, 255, 0, 255, // Green
        0, 0, 255, 255, // Red
        255, 255, 255, 255, // White
    ];
    let page = PrintPage::new(width, height, &bgra).expect("valid page");
    let png = encode_print_page_to_png(page).expect("png encode");

    // Standard 8-byte PNG signature
    assert_eq!(
        &png[0..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );

    // Check IHDR chunk
    assert_eq!(&png[12..16], b"IHDR");
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
    assert_eq!(w, 2);
    assert_eq!(h, 2);
    assert_eq!(png[24], 8); // 8-bit
    assert_eq!(png[25], 6); // RGBA

    // Check IEND at the end
    let len = png.len();
    assert_eq!(&png[len - 8..len - 4], b"IEND");
}

#[test]
fn test_linux_printer_sink_direct_validations() {
    let job_id = PrintJobId::new(42).unwrap();

    // Invalid DPI (0)
    let err_dpi = LinuxPrinterSink::direct(
        job_id,
        0,
        "Printer1",
        PrintOrientation::Portrait,
        PrintDuplex::OneSided,
        1,
    );
    assert!(matches!(err_dpi, Err(PrintError::InvalidDpi(0))));

    // Invalid copies (0)
    let err_copies = LinuxPrinterSink::direct(
        job_id,
        300,
        "Printer1",
        PrintOrientation::Portrait,
        PrintDuplex::OneSided,
        0,
    );
    assert!(matches!(err_copies, Err(PrintError::InvalidCopies(0))));

    // Unavailable CUPS
    let mut runner = MockCommandRunner::new();
    runner.lpstat_r_result = Some(Err(io::ErrorKind::NotFound));
    runner.lp_result = Some(Err(io::ErrorKind::NotFound));
    runner.lpr_result = Some(Err(io::ErrorKind::NotFound));
    let runner_arc = Arc::new(runner);

    let err_unavail = LinuxPrinterSink::direct_with_runner(
        job_id,
        300,
        "Printer1",
        PrintOrientation::Portrait,
        PrintDuplex::OneSided,
        1,
        Some(runner_arc),
    );
    assert!(matches!(err_unavail, Err(PrintError::Unavailable)));
}

#[test]
fn test_linux_printer_sink_rejects_non_existent_printer() {
    let job_id = PrintJobId::new(10).unwrap();
    let mut runner = MockCommandRunner::new();
    runner.lpstat_a_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"Existing_Printer accepting requests\n".to_vec(),
        stderr: Vec::new(),
    }));
    let runner_arc = Arc::new(runner);

    let err = LinuxPrinterSink::direct_with_runner(
        job_id,
        300,
        "Non_Existent_Printer",
        PrintOrientation::Portrait,
        PrintDuplex::OneSided,
        1,
        Some(runner_arc),
    );
    assert!(matches!(err, Err(PrintError::Platform { .. })));
}

#[test]
fn test_linux_printer_sink_full_spool_flow_with_lp() {
    let job_id = PrintJobId::new(101).unwrap();
    let mut runner = MockCommandRunner::new();
    runner.lpstat_a_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"HP_Deskjet accepting requests\n".to_vec(),
        stderr: Vec::new(),
    }));
    let runner_arc = Arc::new(runner);

    let sink = LinuxPrinterSink::direct_with_runner(
        job_id,
        300,
        "HP_Deskjet",
        PrintOrientation::Landscape,
        PrintDuplex::TwoSidedLongEdge,
        2,
        Some(runner_arc.clone()),
    )
    .expect("direct sink");

    assert_eq!(sink.job_id(), job_id);
    assert_eq!(sink.target_dpi(), 300);

    let mut sink_box: Box<dyn PrinterSink> = Box::new(sink);
    sink_box.begin("My Document").expect("begin spool");

    let pixels = [255_u8; 16]; // 2x2 white
    let page = PrintPage::new(2, 2, &pixels).unwrap();
    sink_box.write_page(page).expect("write page 1");
    sink_box.write_page(page).expect("write page 2");

    sink_box.finish().expect("finish spool");

    // Inspect recorded calls to lp
    let calls = runner_arc.recorded_calls();
    let lp_call = calls
        .iter()
        .rfind(|(cmd, _)| cmd == "lp")
        .expect("lp called");
    let args = &lp_call.1;

    assert!(args.contains(&"-d".to_string()));
    assert!(args.contains(&"HP_Deskjet".to_string()));
    assert!(args.contains(&"-n".to_string()));
    assert!(args.contains(&"2".to_string()));
    assert!(args.contains(&"-t".to_string()));
    assert!(args.contains(&"My Document".to_string()));
    assert!(args.contains(&"-o".to_string()));
    assert!(args.contains(&"orientation-requested=4".to_string())); // Landscape
    assert!(args.contains(&"sides=two-sided-long-edge".to_string()));

    // Check that two page files were passed as arguments
    let page_args: Vec<_> = args.iter().filter(|a| a.ends_with(".png")).collect();
    assert_eq!(page_args.len(), 2);
}

#[test]
fn test_linux_printer_sink_fallback_to_lpr() {
    let job_id = PrintJobId::new(202).unwrap();
    let mut runner = MockCommandRunner::new();
    runner.lpstat_a_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"Generic_Printer accepting requests\n".to_vec(),
        stderr: Vec::new(),
    }));
    // lp is missing: NotFound
    runner.lp_result = Some(Err(io::ErrorKind::NotFound));
    let runner_arc = Arc::new(runner);

    let sink = LinuxPrinterSink::direct_with_runner(
        job_id,
        300,
        "Generic_Printer",
        PrintOrientation::Portrait,
        PrintDuplex::OneSided,
        1,
        Some(runner_arc.clone()),
    )
    .expect("direct sink");

    let mut sink_box: Box<dyn PrinterSink> = Box::new(sink);
    sink_box.begin("Fallback Document").expect("begin spool");

    let pixels = [128_u8; 4];
    let page = PrintPage::new(1, 1, &pixels).unwrap();
    sink_box.write_page(page).expect("write page");

    sink_box.finish().expect("finish spool via lpr");

    let calls = runner_arc.recorded_calls();
    let lpr_call = calls
        .iter()
        .rfind(|(cmd, _)| cmd == "lpr")
        .expect("lpr called as fallback");
    let args = &lpr_call.1;

    assert!(args.contains(&"-PGeneric_Printer".to_string()));
    assert!(args.contains(&"-T".to_string()));
    assert!(args.contains(&"Fallback Document".to_string()));
    assert!(args.contains(&"orientation-requested=3".to_string()));
    assert!(args.contains(&"sides=one-sided".to_string()));
}

#[test]
fn test_linux_printer_sink_both_commands_missing_fails_unavailable() {
    let job_id = PrintJobId::new(303).unwrap();
    let mut runner = MockCommandRunner::new();
    runner.lpstat_a_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"Printer1 accepting requests\n".to_vec(),
        stderr: Vec::new(),
    }));
    runner.lp_result = Some(Err(io::ErrorKind::NotFound));
    runner.lpr_result = Some(Err(io::ErrorKind::NotFound));
    let runner_arc = Arc::new(runner);

    let sink = LinuxPrinterSink::direct_with_runner(
        job_id,
        300,
        "Printer1",
        PrintOrientation::Portrait,
        PrintDuplex::OneSided,
        1,
        Some(runner_arc),
    )
    .expect("direct sink");

    let mut sink_box: Box<dyn PrinterSink> = Box::new(sink);
    sink_box.begin("Doc").expect("begin");

    let pixels = [0_u8; 4];
    let page = PrintPage::new(1, 1, &pixels).unwrap();
    sink_box.write_page(page).expect("write page");

    let result = sink_box.finish();
    assert!(matches!(result, Err(PrintError::Unavailable)));
}

#[test]
fn test_linux_printer_dialog_select_flow() {
    let job_id = PrintJobId::new(404).unwrap();
    let mut runner = MockCommandRunner::new();
    runner.lpstat_a_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"Default_Printer accepting requests\nOther_Printer accepting requests\n".to_vec(),
        stderr: Vec::new(),
    }));
    runner.lpstat_d_result = Some(Ok(CommandOutput {
        status_success: true,
        status_code: Some(0),
        stdout: b"system default destination: Default_Printer\n".to_vec(),
        stderr: Vec::new(),
    }));
    let runner_arc = Arc::new(runner);

    let mut dialog = LinuxPrinterDialog::with_runner(runner_arc);
    let page_count = PageCount::new(5).unwrap();

    let selection = dialog
        .select(job_id, page_count)
        .expect("select succeeds")
        .expect("printer selection returned");

    assert_eq!(selection.sink.job_id(), job_id);
    assert_eq!(selection.copies, Copies::default());
    assert_eq!(selection.range, PrintRange::all(page_count));
}

#[test]
fn test_linux_printer_dialog_unavailable_cups_returns_unavailable() {
    let job_id = PrintJobId::new(505).unwrap();
    let mut runner = MockCommandRunner::new();
    runner.lpstat_r_result = Some(Err(io::ErrorKind::NotFound));
    runner.lp_result = Some(Err(io::ErrorKind::NotFound));
    runner.lpr_result = Some(Err(io::ErrorKind::NotFound));
    let runner_arc = Arc::new(runner);

    let mut dialog = LinuxPrinterDialog::with_runner(runner_arc);
    let page_count = PageCount::new(1).unwrap();

    let err = dialog.select(job_id, page_count);
    assert!(matches!(err, Err(PrintError::Unavailable)));
}

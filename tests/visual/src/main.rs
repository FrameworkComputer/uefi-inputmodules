//! Visual regression test runner for UEFI inputmodules app
//!
//! This tool runs the inputmodules UEFI app in QEMU, sends keystrokes to simulate
//! hardware states, captures screenshots, and compares them against baselines.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use image::{ImageReader, Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

/// Visual regression test runner for UEFI inputmodules
#[derive(Parser)]
#[command(name = "visual-test-runner")]
#[command(about = "Run visual regression tests for the UEFI inputmodules app")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all test scenarios
    Run {
        /// Path to scenarios directory
        #[arg(short, long, default_value = "scenarios")]
        scenarios: PathBuf,

        /// Path to baselines directory
        #[arg(short, long, default_value = "baselines")]
        baselines: PathBuf,

        /// Path to outputs directory
        #[arg(short, long, default_value = "outputs")]
        outputs: PathBuf,

        /// Update baselines instead of comparing
        #[arg(short, long)]
        update: bool,

        /// Run only scenarios matching this pattern
        #[arg(short, long)]
        filter: Option<String>,

        /// Path to the built UEFI binary (overrides build_mode)
        #[arg(long)]
        binary: Option<PathBuf>,

        /// Keep QEMU running on failure for debugging
        #[arg(long)]
        keep_on_failure: bool,

        /// Use debug build instead of release (default: release)
        #[arg(short = 'd', long)]
        debug: bool,
    },
    /// List available test scenarios
    List {
        /// Path to scenarios directory
        #[arg(short, long, default_value = "scenarios")]
        scenarios: PathBuf,
    },
}

/// Build mode for the UEFI app
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
enum BuildMode {
    /// Release build (default, optimized)
    #[default]
    Release,
    /// Debug build (with debug info, verbose mode enabled by default)
    Debug,
}

impl std::fmt::Display for BuildMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildMode::Release => write!(f, "release"),
            BuildMode::Debug => write!(f, "debug"),
        }
    }
}

/// A test scenario loaded from YAML
#[derive(Debug, Deserialize, Serialize)]
struct Scenario {
    /// Name of the test scenario
    name: String,

    /// Description of what this test validates
    #[serde(default)]
    description: String,

    /// Build mode for this scenario (release or debug). Defaults to CLI setting.
    #[serde(default)]
    build_mode: Option<BuildMode>,

    /// Steps to execute in order
    steps: Vec<Step>,
}

/// A single step in a test scenario
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action")]
enum Step {
    /// Wait for a specified duration
    #[serde(rename = "wait")]
    Wait {
        /// Milliseconds to wait
        ms: u64,
    },

    /// Send keyboard keys to QEMU
    #[serde(rename = "send_keys")]
    SendKeys {
        /// Keys to send (e.g., ["1", "esc"])
        keys: Vec<String>,
    },

    /// Capture a screenshot and optionally compare to baseline
    #[serde(rename = "screenshot")]
    Screenshot {
        /// Name for the screenshot (without extension)
        name: String,

        /// Whether to compare against baseline (default: true)
        #[serde(default = "default_true")]
        compare: bool,
    },
}

fn default_true() -> bool {
    true
}

/// QMP (QEMU Machine Protocol) client
struct QmpClient {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl QmpClient {
    /// Connect to QMP socket and perform handshake
    fn connect(socket_path: &Path) -> Result<Self> {
        // Wait for socket to appear (QEMU takes time to start)
        for i in 0..60 {
            if socket_path.exists() {
                break;
            }
            if i == 59 {
                bail!("QMP socket did not appear after 30 seconds");
            }
            thread::sleep(Duration::from_millis(500));
        }

        // Small delay to ensure QEMU is ready
        thread::sleep(Duration::from_millis(500));

        let stream = UnixStream::connect(socket_path)
            .with_context(|| format!("Failed to connect to QMP socket: {:?}", socket_path))?;

        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;

        let reader = BufReader::new(stream.try_clone()?);

        let mut client = Self { stream, reader };

        // Read greeting
        let greeting = client.read_response()?;
        if greeting.get("QMP").is_none() {
            bail!("Invalid QMP greeting: {:?}", greeting);
        }

        // Send qmp_capabilities to enter command mode
        client.execute("qmp_capabilities", json!({}))?;

        Ok(client)
    }

    /// Read a JSON response from QMP
    fn read_response(&mut self) -> Result<Value> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        let response: Value = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse QMP response: {}", line))?;
        Ok(response)
    }

    /// Execute a QMP command
    fn execute(&mut self, command: &str, arguments: Value) -> Result<Value> {
        let cmd = if arguments.is_object() && arguments.as_object().unwrap().is_empty() {
            json!({ "execute": command })
        } else {
            json!({ "execute": command, "arguments": arguments })
        };

        let cmd_str = serde_json::to_string(&cmd)? + "\n";
        self.stream.write_all(cmd_str.as_bytes())?;
        self.stream.flush()?;

        // Read response, skipping events
        loop {
            let response = self.read_response()?;
            if response.get("event").is_some() {
                continue; // Skip events
            }
            if let Some(error) = response.get("error") {
                bail!("QMP error: {:?}", error);
            }
            return Ok(response);
        }
    }

    /// Send a key press to QEMU
    fn send_key(&mut self, key: &str) -> Result<()> {
        // Map common key names to QMP key names
        let key_lower = key.to_lowercase();
        let qmp_key = match key_lower.as_str() {
            "esc" | "escape" => "esc",
            "enter" | "return" => "ret",
            "space" => "spc",
            "1" => "1",
            "2" => "2",
            "3" => "3",
            "4" => "4",
            "5" => "5",
            "6" => "6",
            "7" => "7",
            "0" => "0",
            "a" => "a",
            "b" => "b",
            "c" => "c",
            "d" => "d",
            "e" => "e",
            "f" => "f",
            "g" => "g",
            "h" => "h",
            "l" => "l",
            "q" => "q",
            "v" => "v",
            "w" => "w",
            "x" => "x",
            "z" => "z",
            "f12" => "f12",
            "?" => "shift-slash",
            other => other,
        };

        // Handle shift modifier for ?
        // Use hold-time of 100ms to ensure UEFI app registers the keypress
        if qmp_key == "shift-slash" {
            self.execute(
                "send-key",
                json!({
                    "keys": [
                        { "type": "qcode", "data": "shift" },
                        { "type": "qcode", "data": "slash" }
                    ],
                    "hold-time": 100
                }),
            )?;
        } else {
            self.execute(
                "send-key",
                json!({
                    "keys": [{ "type": "qcode", "data": qmp_key }],
                    "hold-time": 100
                }),
            )?;
        }

        // Delay after keypress for the app to process
        thread::sleep(Duration::from_millis(150));

        Ok(())
    }

    /// Capture a screenshot to a file
    fn screendump(&mut self, output_path: &Path) -> Result<()> {
        self.execute(
            "screendump",
            json!({
                "filename": output_path.to_string_lossy()
            }),
        )?;

        // Wait for screenshot to be written
        thread::sleep(Duration::from_millis(200));

        if !output_path.exists() {
            bail!("Screenshot was not created at {:?}", output_path);
        }

        Ok(())
    }

    /// Reset the VM to initial state
    fn system_reset(&mut self) -> Result<()> {
        self.execute("system_reset", json!({}))?;
        Ok(())
    }
}

/// QEMU process manager
struct QemuProcess {
    child: Child,
    qmp_socket: PathBuf,
}

impl QemuProcess {
    /// Start QEMU in headless mode with QMP enabled
    fn start(
        binary_path: &Path,
        ovmf_code: &Path,
        ovmf_vars: &Path,
        work_dir: &Path,
    ) -> Result<Self> {
        let esp_dir = work_dir.join("esp");
        let boot_dir = esp_dir.join("efi/boot");
        fs::create_dir_all(&boot_dir)?;

        // Copy the UEFI binary to the ESP
        fs::copy(binary_path, boot_dir.join("bootx64.efi"))?;

        // Copy OVMF_VARS to work directory (it gets modified)
        let ovmf_vars_copy = work_dir.join("OVMF_VARS.fd");
        fs::copy(ovmf_vars, &ovmf_vars_copy)?;

        let qmp_socket = work_dir.join("qmp.sock");

        // Remove old socket if it exists
        let _ = fs::remove_file(&qmp_socket);

        let child = Command::new("qemu-system-x86_64")
            .args([
                "-enable-kvm",
                "-M",
                "q35",
                "-m",
                "1024",
                "-net",
                "none",
                // Headless display - use VNC server for screendump and keyboard input
                "-display",
                "none",
                "-vnc",
                ":99",
                // QMP socket for control
                "-qmp",
                &format!("unix:{},server,nowait", qmp_socket.display()),
                // Serial output to null (or could capture for debugging)
                "-serial",
                "null",
                // UEFI firmware
                "-drive",
                &format!(
                    "if=pflash,format=raw,readonly=on,file={}",
                    ovmf_code.display()
                ),
                "-drive",
                &format!(
                    "if=pflash,format=raw,readonly=off,file={}",
                    ovmf_vars_copy.display()
                ),
                // ESP with our UEFI app
                "-drive",
                &format!("format=raw,file=fat:rw:{}", esp_dir.display()),
                // Set resolution via device properties
                "-device",
                "VGA,vgamem_mb=64",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to start QEMU")?;

        Ok(Self { child, qmp_socket })
    }

    /// Get path to QMP socket
    fn qmp_socket(&self) -> &Path {
        &self.qmp_socket
    }

    /// Kill the QEMU process
    fn kill(&mut self) -> Result<()> {
        self.child.kill().context("Failed to kill QEMU")?;
        self.child.wait()?;
        Ok(())
    }
}

impl Drop for QemuProcess {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

/// Compare two images pixel by pixel
fn compare_images(actual: &Path, expected: &Path) -> Result<ImageComparison> {
    let actual_img = ImageReader::open(actual)
        .with_context(|| format!("Failed to open actual image: {:?}", actual))?
        .decode()
        .context("Failed to decode actual image")?
        .to_rgb8();

    let expected_img = ImageReader::open(expected)
        .with_context(|| format!("Failed to open expected image: {:?}", expected))?
        .decode()
        .context("Failed to decode expected image")?
        .to_rgb8();

    let (actual_width, actual_height) = actual_img.dimensions();
    let (expected_width, expected_height) = expected_img.dimensions();

    if actual_width != expected_width || actual_height != expected_height {
        return Ok(ImageComparison::SizeMismatch {
            actual: (actual_width, actual_height),
            expected: (expected_width, expected_height),
        });
    }

    let mut diff_pixels = 0u64;
    let total_pixels = (actual_width as u64) * (actual_height as u64);
    let mut diff_image = RgbImage::new(actual_width, actual_height);

    for y in 0..actual_height {
        for x in 0..actual_width {
            let actual_pixel = actual_img.get_pixel(x, y);
            let expected_pixel = expected_img.get_pixel(x, y);

            if actual_pixel != expected_pixel {
                diff_pixels += 1;
                // Highlight differences in red
                diff_image.put_pixel(x, y, Rgb([255, 0, 0]));
            } else {
                // Dim the matching pixels
                let dimmed = Rgb([
                    actual_pixel[0] / 3,
                    actual_pixel[1] / 3,
                    actual_pixel[2] / 3,
                ]);
                diff_image.put_pixel(x, y, dimmed);
            }
        }
    }

    if diff_pixels == 0 {
        Ok(ImageComparison::Match)
    } else {
        Ok(ImageComparison::Mismatch {
            diff_pixels,
            total_pixels,
            diff_image,
        })
    }
}

enum ImageComparison {
    Match,
    SizeMismatch {
        actual: (u32, u32),
        expected: (u32, u32),
    },
    Mismatch {
        diff_pixels: u64,
        total_pixels: u64,
        diff_image: RgbImage,
    },
}

/// Load all scenarios from a directory
fn load_scenarios(dir: &Path) -> Result<Vec<(PathBuf, Scenario)>> {
    let mut scenarios = Vec::new();

    if !dir.exists() {
        bail!("Scenarios directory does not exist: {:?}", dir);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read scenario file: {:?}", path))?;
            let scenario: Scenario = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse scenario file: {:?}", path))?;
            scenarios.push((path, scenario));
        }
    }

    scenarios.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(scenarios)
}

/// Find the UEFI binary for the specified build mode
fn find_binary(
    specified: Option<&Path>,
    project_root: &Path,
    build_mode: BuildMode,
) -> Result<PathBuf> {
    if let Some(path) = specified {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        bail!("Specified binary does not exist: {:?}", path);
    }

    let binary_path = project_root.join(format!(
        "target/x86_64-unknown-uefi/{}/inputmodules.efi",
        build_mode
    ));

    if binary_path.exists() {
        return Ok(binary_path);
    }

    bail!(
        "No UEFI binary found for {} build. Please build with 'make qemu-release' or 'make qemu' first.\n\
         Looked for: {:?}",
        build_mode,
        binary_path
    );
}

/// Find OVMF firmware files
fn find_ovmf(project_root: &Path) -> Result<(PathBuf, PathBuf)> {
    // Check project's ovmf directory
    let ovmf_dir = project_root.join("ovmf");
    let ovmf_code = ovmf_dir.join("OVMF_CODE.fd");
    let ovmf_vars = ovmf_dir.join("OVMF_VARS.fd");

    if ovmf_code.exists() && ovmf_vars.exists() {
        return Ok((ovmf_code, ovmf_vars));
    }

    // Check system paths
    let system_paths = ["/usr/share/OVMF", "/usr/share/edk2/ovmf", "/usr/share/qemu"];

    for sys_path in system_paths {
        let code = Path::new(sys_path).join("OVMF_CODE.fd");
        let vars = Path::new(sys_path).join("OVMF_VARS.fd");
        if code.exists() && vars.exists() {
            return Ok((code, vars));
        }
    }

    bail!(
        "OVMF firmware not found. Please run 'make ovmf/OVMF_CODE.fd' first.\n\
         Or install OVMF system-wide (e.g., 'dnf install edk2-ovmf')"
    );
}

/// Run a single test scenario
fn run_scenario(
    scenario: &Scenario,
    qmp: &mut QmpClient,
    baselines_dir: &Path,
    outputs_dir: &Path,
    update_baselines: bool,
) -> Result<ScenarioResult> {
    let mut results = Vec::new();

    for step in scenario.steps.iter() {
        match step {
            Step::Wait { ms } => {
                thread::sleep(Duration::from_millis(*ms));
            }

            Step::SendKeys { keys } => {
                for key in keys {
                    qmp.send_key(key)
                        .with_context(|| format!("Failed to send key: {}", key))?;
                }
            }

            Step::Screenshot { name, compare } => {
                let output_path = outputs_dir.join(format!("{}.ppm", name));
                let png_path = outputs_dir.join(format!("{}.png", name));
                let baseline_path = baselines_dir.join(format!("{}.png", name));

                // Capture screenshot (QEMU outputs PPM format)
                qmp.screendump(&output_path)?;

                // Convert PPM to PNG for easier handling
                let img = ImageReader::open(&output_path)
                    .with_context(|| format!("Failed to open screenshot: {:?}", output_path))?
                    .decode()
                    .context("Failed to decode screenshot")?;
                img.save(&png_path)
                    .with_context(|| format!("Failed to save PNG: {:?}", png_path))?;

                // Clean up PPM
                let _ = fs::remove_file(&output_path);

                if update_baselines {
                    // Copy to baselines directory
                    fs::copy(&png_path, &baseline_path).with_context(|| {
                        format!("Failed to copy to baseline: {:?}", baseline_path)
                    })?;
                    results.push(StepResult::BaselineUpdated {
                        name: name.clone(),
                        path: baseline_path.clone(),
                    });
                } else if *compare {
                    if !baseline_path.exists() {
                        results.push(StepResult::MissingBaseline {
                            name: name.clone(),
                            actual_path: png_path,
                        });
                    } else {
                        match compare_images(&png_path, &baseline_path)? {
                            ImageComparison::Match => {
                                results.push(StepResult::Passed { name: name.clone() });
                            }
                            ImageComparison::SizeMismatch { actual, expected } => {
                                results.push(StepResult::SizeMismatch {
                                    name: name.clone(),
                                    actual,
                                    expected,
                                    actual_path: png_path,
                                });
                            }
                            ImageComparison::Mismatch {
                                diff_pixels,
                                total_pixels,
                                diff_image,
                            } => {
                                // Save diff image
                                let diff_path = outputs_dir.join(format!("{}_diff.png", name));
                                diff_image.save(&diff_path)?;

                                results.push(StepResult::PixelMismatch {
                                    name: name.clone(),
                                    diff_pixels,
                                    total_pixels,
                                    actual_path: png_path,
                                    diff_path,
                                });
                            }
                        }
                    }
                } else {
                    results.push(StepResult::ScreenshotCaptured {
                        name: name.clone(),
                        path: png_path,
                    });
                }
            }
        }
    }

    let passed = results.iter().all(|r| {
        matches!(
            r,
            StepResult::Passed { .. }
                | StepResult::BaselineUpdated { .. }
                | StepResult::ScreenshotCaptured { .. }
        )
    });

    Ok(ScenarioResult {
        name: scenario.name.clone(),
        passed,
        step_results: results,
    })
}

#[derive(Debug)]
struct ScenarioResult {
    #[allow(dead_code)] // Kept for Debug output
    name: String,
    passed: bool,
    step_results: Vec<StepResult>,
}

#[derive(Debug)]
enum StepResult {
    Passed {
        name: String,
    },
    BaselineUpdated {
        name: String,
        path: PathBuf,
    },
    ScreenshotCaptured {
        name: String,
        path: PathBuf,
    },
    MissingBaseline {
        name: String,
        actual_path: PathBuf,
    },
    SizeMismatch {
        name: String,
        actual: (u32, u32),
        expected: (u32, u32),
        actual_path: PathBuf,
    },
    PixelMismatch {
        name: String,
        diff_pixels: u64,
        total_pixels: u64,
        actual_path: PathBuf,
        diff_path: PathBuf,
    },
}

fn print_step_result(result: &StepResult) {
    match result {
        StepResult::Passed { name } => {
            println!("    {} {}", "✓".green(), name);
        }
        StepResult::BaselineUpdated { name, path } => {
            println!("    {} {} -> {:?}", "↑".blue(), name, path);
        }
        StepResult::ScreenshotCaptured { name, path } => {
            println!("    {} {} -> {:?}", "📷".normal(), name, path);
        }
        StepResult::MissingBaseline { name, actual_path } => {
            println!("    {} {} - no baseline found", "?".yellow(), name);
            println!("      Actual: {:?}", actual_path);
        }
        StepResult::SizeMismatch {
            name,
            actual,
            expected,
            actual_path,
        } => {
            println!("    {} {} - size mismatch", "✗".red(), name);
            println!(
                "      Expected: {}x{}, Actual: {}x{}",
                expected.0, expected.1, actual.0, actual.1
            );
            println!("      Actual: {:?}", actual_path);
        }
        StepResult::PixelMismatch {
            name,
            diff_pixels,
            total_pixels,
            actual_path,
            diff_path,
        } => {
            let percentage = (*diff_pixels as f64 / *total_pixels as f64) * 100.0;
            println!(
                "    {} {} - {} pixels differ ({:.2}%)",
                "✗".red(),
                name,
                diff_pixels,
                percentage
            );
            println!("      Actual: {:?}", actual_path);
            println!("      Diff:   {:?}", diff_path);
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Find project root (go up from tests/visual)
    let current_dir = std::env::current_dir()?;
    let project_root = if current_dir.ends_with("tests/visual") {
        current_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    } else {
        // Either we're at project root or somewhere else - use current dir
        current_dir.clone()
    };

    match cli.command {
        Commands::List { scenarios } => {
            let scenarios_dir = if scenarios.is_absolute() {
                scenarios
            } else {
                project_root.join("tests/visual").join(scenarios)
            };

            let scenarios = load_scenarios(&scenarios_dir)?;

            println!("Available test scenarios:\n");
            for (path, scenario) in scenarios {
                println!(
                    "  {} - {}",
                    path.file_name().unwrap().to_string_lossy().cyan(),
                    scenario.name
                );
                if !scenario.description.is_empty() {
                    println!("    {}", scenario.description.dimmed());
                }
            }
            Ok(())
        }

        Commands::Run {
            scenarios,
            baselines,
            outputs,
            update,
            filter,
            binary,
            keep_on_failure,
            debug,
        } => {
            let test_dir = project_root.join("tests/visual");
            let scenarios_dir = if scenarios.is_absolute() {
                scenarios
            } else {
                test_dir.join(scenarios)
            };
            let baselines_dir = if baselines.is_absolute() {
                baselines
            } else {
                test_dir.join(baselines)
            };
            let outputs_dir = if outputs.is_absolute() {
                outputs
            } else {
                test_dir.join(outputs)
            };

            // Determine default build mode from CLI
            let default_build_mode = if debug {
                BuildMode::Debug
            } else {
                BuildMode::Release
            };

            // Create outputs directory
            fs::create_dir_all(&outputs_dir)?;

            // Load scenarios
            let mut all_scenarios = load_scenarios(&scenarios_dir)?;

            // Filter if requested
            if let Some(ref pattern) = filter {
                all_scenarios.retain(|(path, scenario)| {
                    scenario.name.contains(pattern)
                        || path
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().contains(pattern))
                });
            }

            if all_scenarios.is_empty() {
                bail!("No scenarios found matching filter");
            }

            // Find OVMF
            let (ovmf_code, ovmf_vars) = find_ovmf(&project_root)?;

            println!("{}", "Visual Regression Test Runner".bold());
            println!("OVMF: {:?}", ovmf_code.parent().unwrap());
            println!(
                "Default build: {}",
                if debug {
                    "debug".yellow()
                } else {
                    "release".green()
                }
            );
            println!(
                "Mode: {}",
                if update {
                    "Update baselines".yellow()
                } else {
                    "Compare".green()
                }
            );
            println!();

            let mut total_passed = 0;
            let mut total_failed = 0;
            let mut failed_scenarios = Vec::new();

            // Track current build mode to restart QEMU only when needed
            let mut current_build_mode: Option<BuildMode> = None;
            let mut qemu: Option<QemuProcess> = None;
            let mut qmp: Option<QmpClient> = None;
            let mut _temp_dir: Option<tempfile::TempDir> = None;
            let mut is_first_scenario_for_build = true;

            for (_path, scenario) in &all_scenarios {
                // Determine build mode for this scenario
                let scenario_build_mode = scenario.build_mode.unwrap_or(default_build_mode);

                // Restart QEMU if build mode changed
                if current_build_mode != Some(scenario_build_mode) {
                    // Shutdown existing QEMU if any
                    if let Some(mut q) = qemu.take() {
                        let _ = q.kill();
                    }
                    drop(qmp.take());
                    _temp_dir = None;

                    // Find binary for this build mode
                    let binary_path =
                        find_binary(binary.as_deref(), &project_root, scenario_build_mode)?;

                    println!(
                        "{} {} ({})",
                        "Starting QEMU with".dimmed(),
                        scenario_build_mode.to_string().cyan(),
                        binary_path.display()
                    );

                    // Create new temp directory
                    let new_temp_dir = tempfile::tempdir()?;

                    // Start QEMU
                    let new_qemu = QemuProcess::start(
                        &binary_path,
                        &ovmf_code,
                        &ovmf_vars,
                        new_temp_dir.path(),
                    )?;

                    // Connect to QMP
                    let new_qmp = QmpClient::connect(new_qemu.qmp_socket())?;

                    qemu = Some(new_qemu);
                    qmp = Some(new_qmp);
                    _temp_dir = Some(new_temp_dir);
                    current_build_mode = Some(scenario_build_mode);
                    is_first_scenario_for_build = true;
                }

                let qmp_client = qmp.as_mut().unwrap();

                // Reset VM to clean state (skip for first scenario after QEMU start)
                if !is_first_scenario_for_build {
                    qmp_client.system_reset()?;
                }
                is_first_scenario_for_build = false;

                // Wait for UEFI app to initialize
                thread::sleep(Duration::from_secs(3));

                println!("{} {}", "Running:".bold(), scenario.name.cyan());

                let scenario_failed;
                match run_scenario(scenario, qmp_client, &baselines_dir, &outputs_dir, update) {
                    Ok(result) => {
                        for step_result in &result.step_results {
                            print_step_result(step_result);
                        }

                        if result.passed {
                            total_passed += 1;
                            scenario_failed = false;
                        } else {
                            total_failed += 1;
                            failed_scenarios.push(scenario.name.clone());
                            scenario_failed = true;
                        }
                    }
                    Err(e) => {
                        println!("    {} {}", "✗".red(), e);
                        total_failed += 1;
                        failed_scenarios.push(scenario.name.clone());
                        scenario_failed = true;
                    }
                }

                if keep_on_failure && scenario_failed {
                    println!(
                        "{}",
                        "Keeping QEMU running for debugging (Ctrl+C to exit)".yellow()
                    );
                    if let Some(q) = qemu.take() {
                        std::mem::forget(q);
                    }
                    break;
                }

                println!();
            }

            // Shutdown QEMU
            if let Some(mut q) = qemu {
                let _ = q.kill();
            }

            // Summary
            println!("{}", "=".repeat(50));
            println!(
                "{} {} passed, {} failed",
                "Results:".bold(),
                total_passed.to_string().green(),
                if total_failed > 0 {
                    total_failed.to_string().red()
                } else {
                    total_failed.to_string().green()
                }
            );

            if !failed_scenarios.is_empty() {
                println!("\n{}", "Failed scenarios:".red());
                for name in &failed_scenarios {
                    println!("  - {}", name);
                }
                std::process::exit(1);
            }

            Ok(())
        }
    }
}

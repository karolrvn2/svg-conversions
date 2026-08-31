use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use svg_conversion_wasm::{generate_color_map_document, process_svg_document};

const DEFAULT_PRIMARY_COLOR: &str = "#BBBBBB";
const DEFAULT_TOLERANCE: f64 = 0.3;
const DEFAULT_OUTPUT_MODE: &str = "rgb";

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let started_at = Instant::now();
    let mut arguments = env::args().skip(1);
    let input_dir = arguments.next().ok_or_else(usage)?;
    let output_dir = arguments.next().ok_or_else(usage)?;
    let primary_color = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_PRIMARY_COLOR.to_owned());
    let tolerance = arguments
        .next()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| format!("invalid tolerance: {value}"))
        })
        .transpose()?
        .unwrap_or(DEFAULT_TOLERANCE);
    let output_mode = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_OUTPUT_MODE.to_owned());
    if arguments.next().is_some() {
        return Err(usage());
    }

    let input_dir = PathBuf::from(input_dir);
    let output_dir = PathBuf::from(output_dir);
    if input_dir == output_dir {
        return Err("input_dir and output_dir must be different directories".to_owned());
    }
    if !input_dir.is_dir() {
        return Err(format!(
            "input directory does not exist: {}",
            input_dir.display()
        ));
    }
    if output_mode != "rgb" && output_mode != "css_vars" {
        return Err(format!("unsupported output mode: {output_mode}"));
    }
    if !tolerance.is_finite() || !(0.0..=1.0).contains(&tolerance) {
        return Err("tolerance must be a finite number between 0 and 1".to_owned());
    }

    // Do not canonicalize either path. In particular, `./icons/source` and
    // `./icons/out` stay exactly as supplied by the caller.
    let sources = find_svg_files(&input_dir)?;
    fs::create_dir_all(&output_dir).map_err(io_error)?;
    let mut converted = 0;
    let mut errors = 0;
    for source in sources {
        let relative = match source.strip_prefix(&input_dir) {
            Ok(relative) => relative,
            Err(_) => {
                log_file_error(
                    &mut errors,
                    &source,
                    "failed to derive its relative output path",
                );
                continue;
            }
        };
        let destination = output_dir.join(relative);
        if let Some(parent) = destination.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                log_file_error(&mut errors, &source, &io_error(error));
                continue;
            }
        }
        let svg = match fs::read_to_string(&source) {
            Ok(svg) => svg,
            Err(error) => {
                log_file_error(&mut errors, &source, &io_error(error));
                continue;
            }
        };
        let processed = match process_svg_document(&svg, &primary_color, tolerance, &output_mode) {
            Ok(processed) => processed,
            Err(error) => {
                log_file_error(&mut errors, &source, &error);
                continue;
            }
        };
        if let Err(error) = fs::write(&destination, processed) {
            log_file_error(&mut errors, &source, &io_error(error));
            continue;
        }
        converted += 1;
    }

    if output_mode == "css_vars" {
        let color_map = generate_color_map_document(&primary_color)?;
        fs::write(output_dir.join("color_map.sass"), color_map).map_err(io_error)?;
    }
    println!(
        "[errors: {errors}] Converted {converted} SVG file(s) to {} in {:.2?}",
        output_dir.display(),
        started_at.elapsed()
    );
    if errors > 0 {
        return Err(format!(
            "[errors: {errors}] {errors} SVG file(s) failed; successful conversions were written"
        ));
    }
    Ok(())
}

fn log_file_error(errors: &mut usize, source: &Path, error: &str) {
    *errors += 1;
    eprintln!("[errors: {errors}] {}: {error}", source.display());
}

fn find_svg_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_svg_files(directory, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_svg_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        if path.is_dir() {
            collect_svg_files(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

fn usage() -> String {
    "Usage: svg-conversion <input_dir> <output_dir> [primary_color] [tolerance] [rgb|css_vars]"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_nested_relative_output_path() {
        let input = Path::new("./icons/source");
        let source = input.join("brands/example.svg");
        assert_eq!(
            source.strip_prefix(input).unwrap(),
            Path::new("brands/example.svg")
        );
    }
}

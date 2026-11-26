//! Excel and CSV export of every list. Lists are built as rows of strings by the routes.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_xlsxwriter::{Format, Workbook};

use crate::ApiError;

/// CSV with a BOM so Excel opens Urdu text correctly. A cell a spreadsheet would run as a
/// formula (a name or note starting with `=`, `+`, `-` or `@`) is written as text
/// (`madrasa_core::spreadsheet`).
pub fn csv(headers: &[String], rows: &[Vec<String>]) -> Vec<u8> {
    use madrasa_core::spreadsheet::csv_cell;
    let mut w = csv::Writer::from_writer(Vec::new());
    let _ = w.write_record(headers.iter().map(|h| csv_cell(h).into_owned()));
    for r in rows {
        let _ = w.write_record(r.iter().map(|c| csv_cell(c).into_owned()));
    }
    let mut out = vec![0xEF, 0xBB, 0xBF];
    out.extend(w.into_inner().unwrap_or_default());
    out
}

/// One-sheet workbook with a bold heading row and auto-ish column widths.
pub fn xlsx(sheet: &str, headers: &[String], rows: &[Vec<String>]) -> Result<Vec<u8>, ApiError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    let name: String = sheet.chars().take(31).collect();
    let _ = ws.set_name(&name);
    let bold = Format::new().set_bold();
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, h, &bold)
            .map_err(|_| ApiError::internal())?;
        let width = rows
            .iter()
            .filter_map(|r| r.get(c))
            .map(|v| v.chars().count())
            .chain(std::iter::once(h.chars().count()))
            .max()
            .unwrap_or(10)
            .clamp(8, 48);
        let _ = ws.set_column_width(c as u16, width as f64 + 2.0);
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let cell = ((r + 1) as u32, c as u16);
            if let Ok(n) = v.parse::<f64>() {
                if !v.is_empty() && !v.starts_with('0') || v == "0" {
                    ws.write_number(cell.0, cell.1, n)
                        .map_err(|_| ApiError::internal())?;
                    continue;
                }
            }
            ws.write_string(cell.0, cell.1, v)
                .map_err(|_| ApiError::internal())?;
        }
    }
    let _ = ws.set_freeze_panes(1, 0);
    wb.save_to_buffer().map_err(|_| ApiError::internal())
}

/// One sheet of a workbook built by `workbook`.
pub struct Sheet<'a> {
    pub name: &'a str,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl<'a> Sheet<'a> {
    pub fn text(name: &'a str, headers: &[&str], rows: Vec<Vec<String>>) -> Self {
        Sheet {
            name,
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows,
        }
    }
}

/// A workbook of several sheets, each with a bold heading row. Every cell is written as
/// text: nothing a file or a user typed is ever a formula, and numbers keep their zeros.
pub fn workbook(sheets: &[Sheet<'_>]) -> Result<Vec<u8>, ApiError> {
    let fail = |_| ApiError::internal();
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    for sheet in sheets {
        let ws = wb.add_worksheet();
        let name: String = sheet
            .name
            .chars()
            .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
            .take(31)
            .collect();
        ws.set_name(&name).map_err(fail)?;
        for (c, h) in sheet.headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &bold)
                .map_err(fail)?;
            let width = sheet
                .rows
                .iter()
                .take(500)
                .filter_map(|r| r.get(c))
                .map(|v| v.chars().count())
                .chain(std::iter::once(h.chars().count()))
                .max()
                .unwrap_or(10)
                .clamp(8, 60);
            let _ = ws.set_column_width(c as u16, width as f64 + 2.0);
        }
        for (r, row) in sheet.rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                if !v.is_empty() {
                    ws.write_string((r + 1) as u32, c as u16, v).map_err(fail)?;
                }
            }
        }
        let _ = ws.set_freeze_panes(1, 0);
    }
    wb.save_to_buffer().map_err(fail)
}

/// A download response.
pub fn file_response(file_name: &str, bytes: Vec<u8>) -> Response {
    let mime = if file_name.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else if file_name.ends_with(".pdf") {
        "application/pdf"
    } else {
        "text/csv; charset=utf-8"
    };
    let disposition = format!("attachment; filename=\"{}\"", file_name.replace('"', ""));
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&disposition)
                    .unwrap_or(HeaderValue::from_static("attachment")),
            ),
        ],
        bytes,
    )
        .into_response()
}

/// Build the file for the requested format.
pub fn build(
    sheet: &str,
    format: &str,
    headers: &[String],
    rows: &[Vec<String>],
) -> Result<(String, Vec<u8>), ApiError> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
    match format {
        "csv" => Ok((format!("{sheet}-{stamp}.csv"), csv(headers, rows))),
        "xlsx" | "" => Ok((format!("{sheet}-{stamp}.xlsx"), xlsx(sheet, headers, rows)?)),
        _ => Err(ApiError::validation("Format must be xlsx or csv.")),
    }
}

//! Excel and CSV export of every list. Lists are built as rows of strings by the routes.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_xlsxwriter::{Format, Workbook};

use crate::ApiError;

/// CSV with a BOM so Excel opens Urdu text correctly.
pub fn csv(headers: &[String], rows: &[Vec<String>]) -> Vec<u8> {
    let mut w = csv::Writer::from_writer(Vec::new());
    let _ = w.write_record(headers);
    for r in rows {
        let _ = w.write_record(r);
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

/// A download response.
pub fn file_response(file_name: &str, bytes: Vec<u8>) -> Response {
    let mime = if file_name.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
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

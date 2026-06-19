use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Cursor, Seek, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxWorksheetData {
    pub sheet_name: Option<String>,
    #[serde(default)]
    pub cells: Vec<Vec<XlsxCellData>>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxCellData {
    pub value: Value,
    #[serde(default)]
    pub style: Option<usize>,
}

/// Streaming XLSX writer that incrementally writes rows to a ZIP-backed
/// workbook. This keeps large table exports from accumulating all rows in memory.
pub struct StreamingXlsxWriter<W: Write + Seek> {
    zip: zip::ZipWriter<W>,
    columns: Vec<String>,
    next_row_number: usize,
}

fn estimate_header_widths(columns: &[String]) -> Vec<usize> {
    columns.iter().map(|column| (column.chars().count() + 2).clamp(10, 60)).collect()
}

fn cols_xml(widths: &[usize]) -> String {
    widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            format!("<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"/>", index + 1, index + 1, width)
        })
        .collect()
}

pub(crate) fn header_row_xml(columns: &[String]) -> String {
    format!(
        "<row r=\"1\">{}</row>",
        columns
            .iter()
            .enumerate()
            .map(|(index, column)| cell_xml(Some(&Value::String(column.clone())), 0, index, Some(1)))
            .collect::<String>()
    )
}

pub(crate) fn data_row_xml(row_number: usize, columns: &[String], row: &[Value]) -> String {
    let cells = columns
        .iter()
        .enumerate()
        .map(|(col_index, _)| cell_xml(row.get(col_index), row_number - 1, col_index, None))
        .collect::<String>();
    format!("<row r=\"{row_number}\">{cells}</row>")
}

fn write_zip_entry<W: Write + Seek>(zip: &mut zip::ZipWriter<W>, path: &str, content: &str) -> Result<(), String> {
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(path, options).map_err(|err| err.to_string())?;
    zip.write_all(content.as_bytes()).map_err(|err| err.to_string())
}

pub(crate) fn start_streaming_xlsx_workbook<W: Write + Seek>(
    writer: W,
    sheet_name: Option<&str>,
    columns: &[String],
) -> Result<StreamingXlsxWriter<W>, String> {
    let sheet_names = vec![normalize_sheet_name(sheet_name)];
    let widths = estimate_header_widths(columns);

    let mut zip = zip::ZipWriter::new(writer);
    write_zip_entry(&mut zip, "[Content_Types].xml", &content_types_xml(1))?;
    write_zip_entry(&mut zip, "_rels/.rels", root_rels_xml())?;
    write_zip_entry(&mut zip, "xl/workbook.xml", &workbook_xml(&sheet_names))?;
    write_zip_entry(&mut zip, "xl/_rels/workbook.xml.rels", &workbook_rels_xml(1))?;
    write_zip_entry(&mut zip, "xl/styles.xml", styles_xml())?;

    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("xl/worksheets/sheet1.xml", options).map_err(|err| err.to_string())?;
    let sheet_header = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
            "<sheetViews><sheetView workbookViewId=\"0\">",
            "<pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/>",
            "</sheetView></sheetViews>",
            "<sheetFormatPr defaultRowHeight=\"15\"/>",
            "<cols>{cols}</cols>",
            "<sheetData>"
        ),
        cols = cols_xml(&widths),
    );
    zip.write_all(sheet_header.as_bytes()).map_err(|err| err.to_string())?;
    zip.write_all(header_row_xml(columns).as_bytes()).map_err(|err| err.to_string())?;

    Ok(StreamingXlsxWriter { zip, columns: columns.to_vec(), next_row_number: 2 })
}

impl<W: Write + Seek> StreamingXlsxWriter<W> {
    pub fn write_row(&mut self, row: &[Value]) -> Result<(), String> {
        self.zip
            .write_all(data_row_xml(self.next_row_number, &self.columns, row).as_bytes())
            .map_err(|err| err.to_string())?;
        self.next_row_number += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<W, String> {
        let row_count = self.next_row_number.saturating_sub(1);
        let range = sheet_range(self.columns.len(), row_count);
        self.zip
            .write_all(format!("</sheetData><autoFilter ref=\"{range}\"/></worksheet>").as_bytes())
            .map_err(|err| err.to_string())?;
        self.zip.finish().map_err(|err| err.to_string())
    }
}

pub(crate) fn finish_streaming_xlsx_workbook<W: Write + Seek>(writer: StreamingXlsxWriter<W>) -> Result<W, String> {
    writer.finish()
}

fn escape_xml(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            let code = *ch as u32;
            code == 9 || code == 10 || code == 13 || code >= 32
        })
        .flat_map(|ch| match ch {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect::<Vec<_>>(),
            '"' => "&quot;".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

fn column_name(index: usize) -> String {
    let mut out = String::new();
    let mut n = index + 1;
    while n > 0 {
        let rem = (n - 1) % 26;
        out.insert(0, (b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    out
}

fn cell_ref(row_index: usize, col_index: usize) -> String {
    format!("{}{}", column_name(col_index), row_index + 1)
}

fn sheet_range(column_count: usize, row_count: usize) -> String {
    if column_count == 0 || row_count == 0 {
        return "A1".to_string();
    }
    format!("A1:{}{}", column_name(column_count - 1), row_count)
}

fn normalize_sheet_name(input: Option<&str>) -> String {
    let base = input.unwrap_or("Sheet1");
    let name: String = base
        .chars()
        .map(|ch| match ch {
            '[' | ']' | ':' | '*' | '?' | '/' | '\\' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .trim()
        .to_string();
    let fallback = if name.is_empty() { "Sheet1" } else { &name };
    fallback.chars().take(31).collect()
}

fn display_width(value: &str) -> usize {
    value.chars().map(|ch| if ch.is_ascii() { 1 } else { 2 }).sum()
}

fn estimate_column_widths(columns: &[String], rows: &[Vec<Value>]) -> Vec<usize> {
    let cells = std::iter::once(columns.iter().map(|column| column.as_str()).collect::<Vec<_>>())
        .chain(rows.iter().take(100).map(|row| row.iter().map(value_string).collect::<Vec<_>>()));
    estimate_text_column_widths(cells)
}

fn estimate_cell_column_widths(rows: &[Vec<XlsxCellData>]) -> Vec<usize> {
    estimate_text_column_widths(
        rows.iter().take(100).map(|row| row.iter().map(|cell| value_string(&cell.value)).collect::<Vec<_>>()),
    )
}

fn estimate_text_column_widths<'a>(rows: impl Iterator<Item = Vec<&'a str>>) -> Vec<usize> {
    let mut max_by_column = Vec::<usize>::new();
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            if index >= max_by_column.len() {
                max_by_column.resize(index + 1, 0);
            }
            max_by_column[index] = max_by_column[index].max(display_width(value).min(60));
        }
    }
    max_by_column.into_iter().map(|width| (width + 3).clamp(8, 60)).collect()
}

fn value_string(value: &Value) -> &str {
    match value {
        Value::String(value) => value.as_str(),
        _ => "",
    }
}

fn cell_xml(value: Option<&Value>, row_index: usize, col_index: usize, style: Option<usize>) -> String {
    let reference = cell_ref(row_index, col_index);
    let style_attr = style.map_or(String::new(), |s| format!(" s=\"{s}\""));
    match value {
        Some(Value::Null) | None => format!("<c r=\"{reference}\"{style_attr}/>"),
        Some(Value::Bool(v)) => {
            let bool_v = if *v { 1 } else { 0 };
            format!("<c r=\"{reference}\" t=\"b\"{style_attr}><v>{bool_v}</v></c>")
        }
        Some(Value::Number(n)) => {
            if n.as_f64().is_some_and(|f| f.is_finite()) {
                format!("<c r=\"{reference}\"{style_attr}><v>{}</v></c>", n)
            } else {
                format!(
                    "<c r=\"{reference}\" t=\"inlineStr\"{style_attr}><is><t>{}</t></is></c>",
                    escape_xml(&n.to_string())
                )
            }
        }
        Some(Value::String(s)) => {
            format!("<c r=\"{reference}\" t=\"inlineStr\"{style_attr}><is><t>{}</t></is></c>", escape_xml(s))
        }
        Some(other) => format!(
            "<c r=\"{reference}\" t=\"inlineStr\"{style_attr}><is><t>{}</t></is></c>",
            escape_xml(&other.to_string())
        ),
    }
}

fn worksheet_xml(data: &XlsxWorksheetData) -> String {
    if !data.cells.is_empty() {
        return worksheet_cells_xml(&data.cells);
    }
    let total_rows = data.rows.len() + 1;
    let range = sheet_range(data.columns.len(), total_rows);
    let widths = estimate_column_widths(&data.columns, &data.rows);

    let cols_xml = widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            format!("<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"/>", index + 1, index + 1, width)
        })
        .collect::<String>();

    let header_xml = format!(
        "<row r=\"1\">{}</row>",
        data.columns
            .iter()
            .enumerate()
            .map(|(index, col)| cell_xml(Some(&Value::String(col.clone())), 0, index, Some(1)))
            .collect::<String>()
    );

    let body_xml = data
        .rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let excel_row = row_index + 2;
            let cells = data
                .columns
                .iter()
                .enumerate()
                .map(|(col_index, _)| cell_xml(row.get(col_index), excel_row - 1, col_index, None))
                .collect::<String>();
            format!("<row r=\"{excel_row}\">{cells}</row>")
        })
        .collect::<String>();

    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
            "<dimension ref=\"{range}\"/>",
            "<sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews>",
            "<sheetFormatPr defaultRowHeight=\"15\"/>",
            "<cols>{cols_xml}</cols>",
            "<sheetData>{header_xml}{body_xml}</sheetData>",
            "<autoFilter ref=\"{range}\"/>",
            "</worksheet>"
        ),
        range = range,
        cols_xml = cols_xml,
        header_xml = header_xml,
        body_xml = body_xml,
    )
}

fn worksheet_cells_xml(rows: &[Vec<XlsxCellData>]) -> String {
    let max_cols = rows.iter().map(Vec::len).max().unwrap_or(1);
    let range = sheet_range(max_cols, rows.len().max(1));
    let cols_xml = estimate_cell_column_widths(rows)
        .iter()
        .enumerate()
        .map(|(index, width)| {
            format!("<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"/>", index + 1, index + 1, width)
        })
        .collect::<String>();
    let body_xml = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let excel_row = row_index + 1;
            let cells = row
                .iter()
                .enumerate()
                .map(|(col_index, cell)| cell_xml(Some(&cell.value), row_index, col_index, cell.style))
                .collect::<String>();
            format!("<row r=\"{excel_row}\">{cells}</row>")
        })
        .collect::<String>();

    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
            "<dimension ref=\"{range}\"/>",
            "<sheetViews><sheetView workbookViewId=\"0\"/></sheetViews>",
            "<sheetFormatPr defaultRowHeight=\"15\"/>",
            "<cols>{cols_xml}</cols>",
            "<sheetData>{body_xml}</sheetData>",
            "</worksheet>"
        ),
        range = range,
        cols_xml = cols_xml,
        body_xml = body_xml,
    )
}

fn content_types_xml(sheet_count: usize) -> String {
    let overrides = (1..=sheet_count)
        .map(|index| {
            format!(
                "<Override PartName=\"/xl/worksheets/sheet{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
            )
        })
        .collect::<String>();
    format!(
        concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
        "{}",
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        "</Types>"
        ),
        overrides
    )
}

fn root_rels_xml() -> &'static str {
    concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>",
        "</Relationships>"
    )
}

fn workbook_xml(sheet_names: &[String]) -> String {
    let sheets = sheet_names
        .iter()
        .enumerate()
        .map(|(index, sheet_name)| {
            let sheet_id = index + 1;
            format!("<sheet name=\"{}\" sheetId=\"{sheet_id}\" r:id=\"rId{sheet_id}\"/>", escape_xml(sheet_name))
        })
        .collect::<String>();
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
            "<sheets>{}</sheets>",
            "</workbook>"
        ),
        sheets
    )
}

fn workbook_rels_xml(sheet_count: usize) -> String {
    let sheet_rels = (1..=sheet_count)
        .map(|index| {
            format!(
                "<Relationship Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>"
            )
        })
        .collect::<String>();
    let style_id = sheet_count + 1;
    format!(
        concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "{}",
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
        "</Relationships>"
        ),
        sheet_rels, style_id
    )
}

fn styles_xml() -> &'static str {
    concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        "<fonts count=\"5\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font><font><sz val=\"11\"/><color rgb=\"FFFF0000\"/><name val=\"Calibri\"/></font><font><sz val=\"11\"/><color rgb=\"FFFFC000\"/><name val=\"Calibri\"/></font><font><sz val=\"11\"/><color rgb=\"FF00A6A6\"/><name val=\"Calibri\"/></font></fonts>",
        "<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>",
        "<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>",
        "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
        "<cellXfs count=\"5\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/><xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/><xf numFmtId=\"0\" fontId=\"3\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/><xf numFmtId=\"0\" fontId=\"4\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs>",
        "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>",
        "</styleSheet>"
    )
}

pub fn build_xlsx_workbook(data: &XlsxWorksheetData) -> Result<Vec<u8>, String> {
    build_xlsx_workbook_multi(std::slice::from_ref(data))
}

pub fn build_xlsx_workbook_multi(sheets: &[XlsxWorksheetData]) -> Result<Vec<u8>, String> {
    let sheets = if sheets.is_empty() {
        vec![XlsxWorksheetData {
            sheet_name: Some("Sheet1".to_string()),
            cells: Vec::new(),
            columns: vec!["Result".to_string()],
            rows: vec![vec![Value::String("No data".to_string())]],
        }]
    } else {
        sheets.to_vec()
    };
    let sheet_names = unique_sheet_names(&sheets);
    let mut files = vec![
        ("[Content_Types].xml".to_string(), content_types_xml(sheets.len())),
        ("_rels/.rels".to_string(), root_rels_xml().to_string()),
        ("xl/workbook.xml".to_string(), workbook_xml(&sheet_names)),
        ("xl/_rels/workbook.xml.rels".to_string(), workbook_rels_xml(sheets.len())),
        ("xl/styles.xml".to_string(), styles_xml().to_string()),
    ];
    files.extend(
        sheets
            .iter()
            .enumerate()
            .map(|(index, sheet)| (format!("xl/worksheets/sheet{}.xml", index + 1), worksheet_xml(sheet))),
    );

    let cursor = Cursor::new(Vec::<u8>::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (path, content) in files {
        zip.start_file(path, options).map_err(|err| err.to_string())?;
        zip.write_all(content.as_bytes()).map_err(|err| err.to_string())?;
    }

    let output = zip.finish().map_err(|err| err.to_string())?;
    Ok(output.into_inner())
}

fn unique_sheet_names(sheets: &[XlsxWorksheetData]) -> Vec<String> {
    let mut used = std::collections::HashMap::<String, usize>::new();
    sheets
        .iter()
        .map(|sheet| {
            let base = normalize_sheet_name(sheet.sheet_name.as_deref());
            let count = used.entry(base.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                return base;
            }
            let suffix = format!(" ({count})");
            let max_base_len = 31usize.saturating_sub(suffix.chars().count());
            format!("{}{}", base.chars().take(max_base_len).collect::<String>(), suffix)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{build_xlsx_workbook, start_streaming_xlsx_workbook, XlsxWorksheetData};
    use calamine::{open_workbook_auto, Reader};
    use serde_json::json;
    use std::fs;

    #[test]
    fn builds_xlsx_zip_with_sheet_data() {
        let workbook = build_xlsx_workbook(&XlsxWorksheetData {
            sheet_name: Some("Users".to_string()),
            cells: Vec::new(),
            columns: vec!["id".to_string(), "name".to_string(), "active".to_string()],
            rows: vec![vec![json!(1), json!("Ada & Bob"), json!(true)], vec![json!(2), json!(null), json!(false)]],
        })
        .expect("build workbook");
        let text = String::from_utf8_lossy(&workbook);

        assert_eq!(workbook[0], 0x50);
        assert_eq!(workbook[1], 0x4b);
        assert!(text.contains("[Content_Types].xml"));
        assert!(text.contains("xl/worksheets/sheet1.xml"));
        assert!(text.contains("name=\"Users\""));
        assert!(text.contains("<c r=\"A2\"><v>1</v></c>"));
        assert!(text.contains("Ada &amp; Bob"));
        assert!(text.contains("<c r=\"C2\" t=\"b\"><v>1</v></c>"));
    }

    #[test]
    fn sanitizes_invalid_sheet_name() {
        let workbook = build_xlsx_workbook(&XlsxWorksheetData {
            sheet_name: Some("bad/name:with*chars?and-a-very-long-tail".to_string()),
            cells: Vec::new(),
            columns: vec!["value".to_string()],
            rows: vec![vec![json!("ok")]],
        })
        .expect("build workbook");
        let text = String::from_utf8_lossy(&workbook);
        assert!(text.contains("name=\"bad name with chars and-a-very-\""));
    }

    #[test]
    fn streams_xlsx_rows_to_a_readable_workbook() {
        let path = std::env::temp_dir().join(format!("dbx-stream-test-{}.xlsx", uuid::Uuid::new_v4()));
        {
            let file = fs::File::create(&path).expect("create temp xlsx");
            let mut writer =
                start_streaming_xlsx_workbook(file, Some("Streamed"), &["id".to_string(), "name".to_string()])
                    .expect("start workbook");
            writer.write_row(&[json!(1), json!("Ada")]).expect("write row");
            writer.write_row(&[json!(2), json!("Bob")]).expect("write row");
            drop(writer.finish().expect("finish workbook"));
        }

        let mut workbook = open_workbook_auto(&path).expect("open workbook");
        let range = workbook.worksheet_range("Streamed").expect("read worksheet");
        assert_eq!(range.get_value((0, 0)).expect("header"), &calamine::Data::String("id".to_string()));
        assert_eq!(range.get_value((1, 0)).expect("row1"), &calamine::Data::Float(1.0));
        assert_eq!(range.get_value((2, 1)).expect("row2"), &calamine::Data::String("Bob".to_string()));
        let _ = fs::remove_file(&path);
    }
}

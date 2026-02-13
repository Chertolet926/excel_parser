use std::fs::File;

mod excel_parser;
use excel_parser::{ZipFs, FilterSet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("sample_0.xlsx")?;
    let limit_bytes: u64 = 100 * 1024 * 1024;  // 100 MiB
    
    let filter = FilterSet::new()
        .add_exact("xl/workbook.xml")?
        .add_glob("xl/worksheets/*.xml")?
        .add_glob("xl/sharedStrings.xml")?;

    let fs_limited = ZipFs::new(file, Some(filter), Some(limit_bytes))?;
    
    for file_obj in fs_limited.list_files("xl/worksheets") {
        println!("{}", file_obj);
    }

    Ok(())
}
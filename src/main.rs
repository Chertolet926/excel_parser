use std::fs::File;
use std::time::Instant;

mod excel_parser;
use excel_parser::{ZipFs, FilterSet, ZipFsError, SharedStrings};

struct ExcelParser {
    excel_fs: ZipFs,
    shared_strings: Option<SharedStrings>,
}

impl ExcelParser {
    pub fn new(excel_file: File, size_limit: u64) -> Result<Self, ZipFsError> {
        let filters = FilterSet::new()
            .add_exact("xl/sharedStrings.xml")?
            .add_glob("xl/worksheets/*.xml")?;

        let fs = ZipFs::new(
            excel_file,
            Some(filters),
            Some(size_limit))?; 

        Ok(ExcelParser { excel_fs: fs, shared_strings: None })
    }

    pub fn parse(&mut self) -> Result<(), ZipFsError> {
        Ok(())
    }

    /// Parse shared strings from the Excel file
    pub fn parse_shared_strings(&mut self) -> Result<(), ZipFsError> {
        let start = Instant::now();
        
        if let Some(content) = self.excel_fs.get_file("xl/sharedStrings.xml") {
            match SharedStrings::load(content) {
                Ok(s) => self.shared_strings = Some(s),
                Err(e) => return Err(ZipFsError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to parse shared strings: {}", e)))),
            }
        }
        
        let elapsed = start.elapsed();
        eprintln!("[BENCH] Shared strings parsing: {} ms ({} strings)", 
            elapsed.as_millis(), 
            self.shared_strings.as_ref().map(|s| s.len()).unwrap_or(0));
        
        Ok(())
    }
}

fn run_fuzzy_search(shared: &SharedStrings, query: &str, threshold: i64) {
    let start = Instant::now();
    let results = shared.fuzzy_find(query, threshold);
    let elapsed = start.elapsed();
    
    println!("\n🔍 Fuzzy search for \"{}\" (threshold: {}):", query, threshold);
    println!("   Found {} matches in {:.2?}", results.len(), elapsed);
    
    if results.is_empty() {
        println!("   No matches found.");
    } else {
        // Show top 10 results
        for (i, (idx, score)) in results.iter().take(10).enumerate() {
            if let Some(s) = shared.get(*idx) {
                println!("   [{}] (score: {:4}) {}", i, score, s);
            }
        }
        if results.len() > 10 {
            println!("   ... and {} more", results.len() - 10);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("sample_0.xlsx")?;
    let limit_bytes: u64 = 100 * 1024 * 1024;  // 100 MiB
    let mut parser = ExcelParser::new(file, limit_bytes)?;    
    parser.parse()?;
    
    // Parse shared strings with benchmark
    parser.parse_shared_strings()?;
    
    // Get shared strings reference
    let shared = match parser.shared_strings {
        Some(ref s) => s,
        None => {
            eprintln!("No shared strings found!");
            return Ok(());
        }
    };

    println!("\n📊 Loaded {} shared strings", shared.len());
    
    // Run fuzzy search tests
    println!("\n========================================");
    println!("         FUZZY SEARCH TESTS");
    println!("========================================");
    
    // Test 1: Search for "Курс"
    run_fuzzy_search(shared, "Курс", 0);
    
    // Test 2: Search for "Суббота"
    run_fuzzy_search(shared, "Суббота", 0);
    
    // Test 3: Search for "Теория функций комплексной переменной"
    run_fuzzy_search(shared, "Теория функций комплексной переменной", 0);
    
    // Performance benchmark
    println!("\n========================================");
    println!("         PERFORMANCE BENCHMARK");
    println!("========================================");
    
    let queries = [
        "Курс",
        "Суббота", 
        "Теория функций комплексной переменной (пр) Головин Е.Д. 3-7",
        "математика",
        "лекция",
    ];
    
    let iterations = 100;
    
    for query in &queries {
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = shared.fuzzy_find(query, 0);
        }
        let elapsed = start.elapsed();
        let avg_time_us = elapsed.as_micros() as f64 / iterations as f64;
        println!("   Query \"{}\": {:.2} μs avg ({} iterations)", query, avg_time_us, iterations);
    }
    
    println!("\n========================================");
    println!("         TESTS COMPLETED");
    println!("========================================");

    Ok(())
}

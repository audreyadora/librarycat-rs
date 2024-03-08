use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Arc};
use std::panic;
use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use pdf_extract;
use rand;
use regex::Regex;
use serde::{Deserialize, Serialize};
use stop_words::{get, LANGUAGE};
use unicode_segmentation::UnicodeSegmentation;
use keyword_extraction::tf_idf::{TfIdf, TfIdfParams};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Document {
    filename: String,
    keywords: Vec<String>,
}

impl Document {
    fn new(filename: String, keywords: Vec<String>) -> Self {
        Document { filename, keywords }
    }
}

fn load_tag_exclusions(file_path: &str) -> Result<Vec<String>, csv::Error> {
    let mut reader = csv::ReaderBuilder::new().from_path(file_path)?;

    let mut tag_exclusions = Vec::new();

    for result in reader.records() {
        match result {
            Ok(record) => {
                if let Some(tag) = record.get(0) {
                    tag_exclusions.push(tag.to_string());
                }
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(tag_exclusions)
}

fn exclude_tags(content: String, exclusions: &[String]) -> String {
    let lower_content = content.to_lowercase();
    let exclusion_patterns: Vec<String> = exclusions
        .iter()
        .flat_map(|e| vec![e.clone(), e.clone() + "s"])
        .map(|e| e.trim().to_lowercase())
        .collect();
    let regex_pattern = format!("\\b(?:{})\\b", exclusion_patterns.join("|"));
    let regex = Regex::new(&regex_pattern).expect("Invalid regex pattern");

    regex.replace_all(&lower_content, "").to_string()
}

fn strip_xml_tags(input: &str) -> String {
    let tag_pattern = Regex::new(r"<[^>]+>").unwrap();
    tag_pattern.replace_all(input, "").into_owned()
}

fn capitalize_first_letter(input: &str) -> String {
    let mut iter = input.graphemes(true);

    if let Some(first_grapheme) = iter.next() {
        let rest_of_string: String = iter.collect();
        let capitalized_first_grapheme = first_grapheme.to_uppercase();
        capitalized_first_grapheme + &rest_of_string
    } else {
        input.to_string()
    }
}

fn post_proc_keywords(input: Vec<String>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();

    for s in input {
        let trimmed_string = s.trim();

        if let Ok(_) = trimmed_string.parse::<f64>() {
            if trimmed_string.len() == 4 {
                result.push(trimmed_string.to_string());
            }
        } else if UnicodeSegmentation::graphemes(trimmed_string, true).count() >= 3 && !trimmed_string.is_empty() {
            let capitalized_string = capitalize_first_letter(trimmed_string);
            result.push(capitalized_string);
        }
    }

    result
}

fn collect_resources_into_string<P: AsRef<Path>>(path: P) -> Result<String, Box<dyn std::error::Error>> {
    let file = fs::File::open(&path)?;
    let mut archive = epub::archive::EpubArchive::from_reader(BufReader::new(file))?;

    let mut result = String::new();
    for file_name in archive.files.clone() {
        if let Ok(content) = archive.get_entry_as_str(&file_name) {
            let content_without_tags = strip_xml_tags(&content);
            result.push_str(&content_without_tags);
        }
    }

    Ok(result)
}

fn generate_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros();

    let random_number: u64 = rand::random();

    format!("{}_{}", timestamp, random_number)
}

fn generate_meta(filename: &str, cleaned_content_clone: &str) -> Document {
    let stop_words = get(LANGUAGE::English);
    let punctuation: Vec<String> = [
        ".", ",", ":", ";", "!", "?", "(", ")", "[", "]", "{", "}", "\"", "'", "-",
        "’", "‘", "–",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let tag_exclusions = load_tag_exclusions("src/tag_exclusions.csv").unwrap_or_else(|e| {
        eprintln!("Error loading tag exclusions: {}", e);
        Vec::new()
    });

    let cleaned_content = cleaned_content_clone.to_string();
    let cleaned_content_filtered = exclude_tags(cleaned_content, &tag_exclusions);

    let binding = [cleaned_content_filtered];
    let params = TfIdfParams::UnprocessedDocuments(&binding, &stop_words, Some(&punctuation));
    let tf_idf = TfIdf::new(params);

    let ranked_keywords_tf: Vec<String> = post_proc_keywords(tf_idf.get_ranked_words(50));

    Document::new(filename.to_string(), ranked_keywords_tf)
}

fn process_directory(
    directory_path: &Path,
    recursive: bool,
) -> Result<(HashMap<String, Document>, Vec<String>), Box<dyn Error>> {
    let documents = Arc::new(Mutex::new(HashMap::<String, Document>::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));

    let result = panic::catch_unwind(|| {
        for entry_result in fs::read_dir(directory_path)? {
            let entry = match entry_result {
                Ok(e) => e,
                Err(err) => {
                    errors.lock().unwrap().push(format!("Error reading directory entry: {}", err));
                    continue;
                }
            };

            let file_path = entry.path();
            let file_name = match file_path.file_name().and_then(|n| n.to_str().map(String::from)) {
                Some(name) => name,
                None => {
                    errors.lock().unwrap().push("Invalid file name".to_string());
                    continue;
                }
            };

            if file_name.ends_with(".pdf") {
                if let Err(err) = handle_pdf_file(&documents, &errors, file_path.clone(), file_name.clone()) {
                    errors.lock().unwrap().push(err);
                }
            }

            if file_name.ends_with(".epub") {
                if let Err(err) = handle_epub_file(&documents, &errors, file_path.clone(), file_name.clone()) {
                    errors.lock().unwrap().push(err);
                }
            }

            if recursive && file_path.is_dir() {
                if let Err(err) = process_subdirectory(&documents, &errors, file_path, recursive) {
                    errors.lock().unwrap().push(err);
                }
            }
        }

        Ok::<(), Box<dyn Error>>(())
    });

    if let Err(panic) = result {
        eprintln!("Panic occurred: {:?}", panic);
    }

    let cloned_documents = documents.lock().unwrap().clone();
    let cloned_errors = errors.lock().unwrap().clone();
    Ok((cloned_documents, cloned_errors))
}

fn handle_pdf_file(
    documents: &Arc<Mutex<HashMap<String, Document>>>,  // Adjusted type here
    errors: &Arc<Mutex<Vec<String>>>,
    file_path: PathBuf,
    file_name: String,
) -> Result<(), String> {
    let bytes = match fs::read(&file_path) {
        Ok(b) => b,
        Err(err) => return Err(format!("Error reading PDF file {}: {}", file_name, err)),
    };

    let pdf_content = match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(c) => c,
        Err(err) => return Err(format!("Error extracting text from PDF {}: {}", file_name, err)),
    };

    let document = generate_meta(&file_name, &pdf_content);
    documents.lock().unwrap().insert(generate_id(), document);

    Ok(())
}


fn handle_epub_file(
    documents: &Arc<Mutex<HashMap<String, Document>>>,
    errors: &Arc<Mutex<Vec<String>>>,
    file_path: PathBuf,
    file_name: String,
) -> Result<(), String> {
    let epub_content = match collect_resources_into_string(&file_path) {
        Ok(c) => c,
        Err(err) => return Err(format!("Error collecting resources from EPUB {}: {}", file_name, err)),
    };
    
    let document = generate_meta(&file_name, &epub_content);
    documents.lock().unwrap().insert(generate_id(), document);

    Ok(())
}

fn process_subdirectory(
    documents: &Arc<Mutex<HashMap<String, Document>>>,
    errors: &Arc<Mutex<Vec<String>>>,
    subdirectory_path: PathBuf,
    recursive: bool,
) -> Result<(), String> {
    match process_directory(&subdirectory_path, recursive) {
        Ok((subdir_documents, subdir_errors)) => {
            documents.lock().unwrap().extend(subdir_documents);
            errors.lock().unwrap().extend(subdir_errors);
        }
        Err(err) => return Err(err.to_string()),
    }
    Ok(())
}


fn update_metadata(file_path: &Path, file_name: &str, content: &str) -> Result<(), Box<dyn Error>> {
    // Assume metadata file is in the same directory with ".metadata.json" extension
    let metadata_path = file_path.with_extension(".metadata.json");

    if metadata_path.exists() {
        let metadata_content = fs::read_to_string(&metadata_path)?;

        let mut metadata: serde_json::Value = serde_json::from_str(&metadata_content)?;

        // Generate keywords and update the "tags" array
        let keywords = generate_meta(file_name, content).keywords;
        metadata["tags"] = serde_json::to_value(&keywords)?;

        // Write the updated metadata back to the file
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;
    }

    Ok(())
}

fn main() {
    let path = Path::new("src/test/");
    let recursive = true;

    match process_directory(path, recursive) {
        Ok((documents, errors)) => {
            // Serialize the Documents struct to JSON
            let json = serde_json::to_string_pretty(&documents).unwrap();
            if let Err(err) = fs::write("src/documents.json", json) {
                eprintln!("Error writing to documents.json: {}", err);
            }

            // Print errors
            for error in errors {
                eprintln!("Error: {}", error);
            }
        }
        Err(err) => eprintln!("Error processing directory: {}", err),
    }
}

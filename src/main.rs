use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::BufReader;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use csv::ReaderBuilder;
use epub::archive::EpubArchive;
use pdf_extract;

use rand;
use regex::Regex;
use serde::{Deserialize, Serialize};
use stop_words::{get, LANGUAGE};
use unicode_segmentation::UnicodeSegmentation;
use keyword_extraction::tf_idf::{TfIdf, TfIdfParams};

#[derive(Debug, Serialize, Deserialize)]
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
    let mut reader = ReaderBuilder::new().from_path(file_path)?;

    let mut tag_exclusions = Vec::new();

    for result in reader.records() {
        match result {
            Ok(record) => {
                if let Some(tag) = record.get(0) {
                    tag_exclusions.push(tag.to_string());
                }
            }
            Err(err) => return Err(err),
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
    // Define a regex pattern to match XML tags
    let tag_pattern = Regex::new(r"<[^>]+>").unwrap();
    // Replace all matches with an empty string
    tag_pattern.replace_all(input, "").into_owned()
}

fn capitalize_first_letter(input: &str) -> String {
    // Use graphemes to handle multi-byte characters correctly
    let mut iter = input.graphemes(true);
    
    if let Some(first_grapheme) = iter.next() {
        let rest_of_string: String = iter.collect();
        let capitalized_first_grapheme = first_grapheme.to_uppercase();
        capitalized_first_grapheme + &rest_of_string
    } else {
        input.to_string()  // Return the original string if it's empty
    }
}

fn post_proc_keywords(input: Vec<String>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();

    for s in input {
        let trimmed_string = s.trim();

        if let Ok(_) = trimmed_string.parse::<f64>() {
            // If the string is numeric and exactly 4 characters long, add it to the result.
            if trimmed_string.len() == 4 {
                result.push(trimmed_string.to_string());
            }
        } else if UnicodeSegmentation::graphemes(trimmed_string, true).count() >= 3 && !trimmed_string.is_empty() {
            // If the string is non-numeric, capitalize the first letter and add to the result.
            let capitalized_string = capitalize_first_letter(trimmed_string);
            result.push(capitalized_string);
        }
        // Ignore strings with length less than 3 or empty strings.
    }

    result
}

fn collect_resources_into_string<P: AsRef<Path>>(path: P) -> Result<String, Box<dyn std::error::Error>> {
    let file = fs::File::open(&path)?;
    let mut archive = EpubArchive::from_reader(BufReader::new(file))?;

    // Collect only text resources into a single string
    let mut result = String::new();
    for file_name in archive.files.clone() {
        if let Ok(content) = archive.get_entry_as_str(&file_name) {
            // Strip XML tags using regex
            let content_without_tags = strip_xml_tags(&content);
            // Collect only valid UTF-8 sequences
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

    // Load tag exclusions from CSV
    let tag_exclusions = load_tag_exclusions("src/tag_exclusions.csv").unwrap_or_else(|e| {
        eprintln!("Error loading tag exclusions: {}", e);
        Vec::new()  // Provide an empty vector in case of an error
    });

    // Create a named variable for the cleaned content
    let cleaned_content = cleaned_content_clone.to_string();

    // Exclude tags based on tag_exclusions
    let cleaned_content_filtered = exclude_tags(cleaned_content, &tag_exclusions);

    let binding = [cleaned_content_filtered];
    let params = TfIdfParams::UnprocessedDocuments(&binding, &stop_words, Some(&punctuation));
    let tf_idf = TfIdf::new(params);

    let ranked_keywords_tf: Vec<String> = post_proc_keywords(tf_idf.get_ranked_words(50));

    Document::new(filename.to_string(), ranked_keywords_tf)
}

fn process_directory(directory_path: &Path, recursive: bool) -> Result<HashMap<String, Document>, Box<dyn Error>> {
    let mut documents = HashMap::new();

    for entry_result in fs::read_dir(directory_path)? {
        let entry = entry_result.map_err(|e| {
            eprintln!("Error reading directory entry: {}", e);
            Box::new(e) as Box<dyn Error>
        })?;

        let file_path = entry.path();
        let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();

        if file_name.ends_with(".pdf") {
            println!("PDF Name: {}", file_name.clone());
            let bytes = fs::read(&file_path)?;
            let pdf_content = pdf_extract::extract_text_from_mem(&bytes)?;
            documents.insert(generate_id(), generate_meta(&file_name, &pdf_content));
        }

        if file_name.ends_with(".epub") {
            println!("Epub Name: {}", file_name.clone());
            let epub_content = collect_resources_into_string(&file_path)?;
            documents.insert(generate_id(), generate_meta(&file_name, &epub_content)); 
        }

        if recursive && file_path.is_dir() {
            let subdir_documents = process_directory(&file_path, recursive)?;
            documents.extend(subdir_documents);
        }
    }

    Ok(documents)
}

fn main() {
    let path = Path::new("src/test");
    let recursive = true;

    match process_directory(path, recursive) {
        Ok(documents) => {
            // Serialize the Documents struct to JSON
            let json = serde_json::to_string_pretty(&documents).unwrap();
            fs::write("src/documents.json", json).expect("Unable to write to file");
        }
        Err(err) => eprintln!("Error processing directory: {}", err),
    }
}

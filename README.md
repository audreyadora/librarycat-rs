# librarycat-rs

**librarycat-rs** is a Rust script designed to help you scrape keywords for your personal library of EPUB and PDF research papers and books, facilitating categorization and organization. The script utilizes TF-IDF (Term Frequency-Inverse Document Frequency) for keyword extraction and outputs the results in a JSON file.

## Features

- **EPUB and PDF Support:** Handles both EPUB and PDF formats, extracting relevant text content for analysis.
- **TF-IDF Keyword Extraction:** Utilizes TF-IDF to identify and rank keywords based on their importance in each document.
- **Keyword Post-processing:** Cleans and refines extracted keywords for better categorization. Extracts 4-digit dates as well. 

## Usage

1. Ensure you have Rust installed on your system.
2. Clone the repository: `git clone https://github.com/audreyadora/librarycat-rs.git`
3. Navigate to the project directory: `cd librarycat-rs`
4. Load a copy of your ePub and PDF library under src/test or change the directory in main.rs
5. Run the script: `cargo run`

By default, the script processes the "src/test" directory recursively, extracting keywords from EPUB and PDF files. The results are then saved in a `documents.json` file located in the "src" directory. Implement the keywords in your asset management program or database of choice. 

## Components

- **main.rs:** Collects keywords and outputs the results in `documents.json`.
- **mainii.rs:** A script designed to work with the Eagle Asset Management app, loading keywords into the `metadata.json` tags array for each document.

## Configuration

- **tag_exclusions.csv:** Customize tag exclusions to enhance the accuracy of keyword extraction. Update this CSV file as needed.

## Dependencies

- **csv:** CSV parsing library for reading tag exclusions.
- **epub:** EPUB parsing library for extracting content from EPUB files.
- **pdf_extract:** PDF extraction library for obtaining text content from PDF files.
- **rand:** Provides random number generation for unique document IDs.
- **regex:** Regular expression library for text processing.
- **serde:** Serialization and deserialization library for JSON.
- **stop_words:** Provides common stop words for filtering out non-informative keywords.
- **unicode_segmentation:** Handles Unicode graphemes for correct capitalization.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

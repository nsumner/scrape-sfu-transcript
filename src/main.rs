#![warn(clippy::all,clippy::pedantic,)]

use std::collections::BTreeMap;
use std::io::{Error, ErrorKind};

use clap::Parser;
use lopdf::content::{Content, Operation};
use lopdf::Document;
use lopdf::Error as LopdfError;
use lopdf::Object;
use lopdf::Result as LopdfResult;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Chunk {
    Chunks(Vec<Chunk>),
    String(String),
}
impl Chunk {
    fn get_contained(&self) -> Option<&[Self]> {
        match self {
            Self::String(_) => None,
            Self::Chunks(v) => Some(v.as_slice()),
        }
    }

    fn get_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            Self::Chunks(_) => None,
        }
    }

    const fn is_chunks(&self) -> bool {
        matches!(self, Self::Chunks(_))
    }

    // Simplification recursively transforms Chunks objects containing one
    // element into the single element they contain for readability. Column
    // structure is preserved because that can be useful for ensuring
    // consistency when extracting the data later.
    fn simplify(self) -> Self {
        match self {
            Self::String(s) => Self::String(s.trim().to_string()),
            Self::Chunks(v) => {
                let fresh: Vec<Self> = v.into_iter().map(Self::simplify).collect();
                if fresh.len() == 1 {
                    fresh.into_iter().next().unwrap()
                } else {
                    Self::Chunks(fresh)
                }
            }
        }
    }
}

fn objects_to_chunk(encoding: Option<&str>, operands: &[Object]) -> Chunk {
    let mut chunks = Vec::with_capacity(operands.len());
    for operand in operands {
        match operand {
            Object::String(bytes, _) => {
                chunks.push(Chunk::String(Document::decode_text(encoding, bytes)));
            }
            Object::Array(arr) => {
                chunks.push(objects_to_chunk(encoding, arr));
            }
            _ => {}
        }
    }
    Chunk::Chunks(chunks)
}

fn block_to_chunk(
    operations: &[Operation],
    encodings: &BTreeMap<Vec<u8>, &str>,
) -> LopdfResult<Chunk> {
    let mut current_encoding = None;
    let mut chunks = Vec::new();
    for operation in operations {
        match operation.operator.as_ref() {
            "Tf" => {
                let current_font = operation
                    .operands
                    .first()
                    .ok_or_else(|| LopdfError::Syntax("missing font operand".to_string()))?
                    .as_name()?;
                current_encoding = encodings.get(current_font).copied();
            }
            "Tj" | "TJ" => {
                chunks.push(objects_to_chunk(current_encoding, &operation.operands));
            }
            _ => {}
        }
    }
    Ok(Chunk::Chunks(chunks))
}

fn group_text_blocks(content: &Content) -> Vec<&[Operation]> {
    content
        .operations
        .as_slice()
        .split(|o| matches!(o.operator.as_ref(), "ET"))
        .collect()
}

fn extract_page_chunks(doc: &Document) -> LopdfResult<Vec<Vec<Chunk>>> {
    let mut page_chunks = Vec::new();
    for page_id in doc.get_pages().values().copied() {
        // The first stage per page extracts general page information
        // required to extract the text later.
        let fonts = doc.get_page_fonts(page_id);
        let encodings: BTreeMap<Vec<u8>, &str> = fonts
            .into_iter()
            .map(|(name, font)| (name, font.get_font_encoding()))
            .collect::<BTreeMap<Vec<u8>, &str>>();
        let content_data = doc.get_page_content(page_id)?;
        let content = Content::decode(&content_data)?;

        // After extracting general page information, we can proceed to the
        // text extraction itself.
        let blocks = group_text_blocks(&content);
        let as_chunks: LopdfResult<Vec<Chunk>> = blocks
            .iter()
            .map(|b| block_to_chunk(b, &encodings))
            .collect();
        page_chunks.push(as_chunks?);
    }
    Ok(page_chunks)
}

const FOOTER_BANNER: &str = "S I M O N   F R A S E R   U N I V E R S I T Y";

fn combine_page_chunks(mut page_chunks: Vec<Vec<Chunk>>) -> Result<Vec<Chunk>, Error> {
    let num_pages = page_chunks.len();
    for page in &mut page_chunks[0..num_pages - 1] {
        // The footer starts 7 indices before the end of every page
        // except for the last page, but we leave it on the last page anyway.
        let footer_start = page.len() - 7;
        match &page[footer_start] {
            Chunk::Chunks(v) if v[0] == Chunk::String(String::from(FOOTER_BANNER)) => {}
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Footer banner not found at expected position",
                ));
            }
        }
        page.truncate(footer_start);
    }
    Ok(page_chunks.into_iter().flatten().collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Plan {
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Course {
    subject: String,
    id: String,
    grade: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Transfer {
    course: Course,
    school: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Semester {
    year: String,
    term: String,
    is_good_standing: bool,
    courses: Vec<Course>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StudentInfo {
    id: String,
    plan: Plan,
    transfers: Vec<Transfer>,
    semesters: Vec<Semester>,
}

fn process_plan(plan_chunk: &Chunk) -> Result<Plan, Error> {
    if let Chunk::Chunks(v) = plan_chunk {
        // The standard plan IDs seem to be in the second to last chunk of
        // the block.
        if let Some(s) = v[v.len() - 2].get_string() {
            return Ok(Plan {
                name: s.to_string(),
            });
        }
    }
    Err(Error::new(ErrorKind::InvalidData, "Bad plan chunk found"))
}

const QUALIFIERS: [&str; 3] = ["W", "Q", "Online"];
const BREADTH_TAGS: [&str; 3] = ["B-Sci", "B-Hum", "B-Soc"];

fn matches_breadth(s: &str) -> bool {
    BREADTH_TAGS.iter().any(|b| s.contains(b))
}

fn is_qualifier(s: &str) -> bool {
    QUALIFIERS.contains(&s)
}

fn is_perm_dt(s: &str) -> bool {
    s == "Perm.Dt:" || s.split("-").count() == 3
}

const POSSIBLE_GRADES: [&str; 28] = [
    // Standard passing grades
    "A+", "A", "A-",
    "B+", "B", "B-",
    "C+", "C", "C-",
    "D",
    "P",
    // Temporary grades
    "DE", "GN", "IP",
    // Forms of failing
    "F", "FD", "N",
    // Notations
    "AE", "AU", "CC", "CF", "CN", "CR", "FX", "NC", "WD", "WE", "TR",
];

// NOTE: The unwraps in the transfer and semester processing should e left in
// at least for now. As the data cleaning involves some reverse engineering,
// they help the process fail fast and identify errors.

fn process_transfers(chunks: &[Chunk]) -> Vec<Transfer> {
    // Transform the Chunk sequence into a list of string rows.
    // We can skip over the initial sequence of single string elements,
    // as they contain no transfer information.
    let mut sources = chunks
        .iter()
        .filter(|c| c.is_chunks())
        .filter_map(|c| c.get_contained())
        .map(|slice| {
            slice
                .iter()
                .filter_map(|c| c.get_string())
                // WQB Qualifiers create extra columns in anyy row, so identifying
                // and filtering them evens out the data.
                .filter(|s| !is_qualifier(s) && !matches_breadth(s))
                .collect::<Vec<&str>>()
        })
        .collect::<Vec<Vec<&str>>>();

    // The first row includes a column from the header but actually needs
    // another spacer element in order to align with the other rows nicely.
    sources[0].insert(0, "");

    // Page breaks add a column and split a row into two.
    let page_break_tag = "SFUSR";
    let mut i = 0;
    while i < sources.len() - 1 {
        let position = sources[i].len() - 1;
        if sources[i][position].starts_with(page_break_tag) {
            sources[i].remove(position);
            let next = sources.remove(i + 1);
            sources[i].extend_from_slice(&next);
        }
        i += 1;
    }

    let mut transfers = Vec::with_capacity(chunks.len());

    // By default, the rows are ragged, and individual transfer credits are
    // each split over 2 rows. Extract the course and institution if possible
    // to create `Transfer`s.
    i = 0;
    while i < sources.len() - 1 {
        // Institution names are on the following rows when present.
        // Lines with institution names have 10 columns.
        let school = if [10, 2].contains(&sources[i + 1].len()) {
            Some(sources[i + 1][1].to_string())
        } else {
            None
        };
        let course_offset = usize::from(sources[i].len() == 10);
        // Sanity check that the grades are in the possible grades list
        // to help identify any irregularities in the PDF stream
        // while reverse engineering.
        assert!(POSSIBLE_GRADES.contains(&sources[i][course_offset + 6]));
        transfers.push(Transfer {
            course: Course {
                subject: sources[i][course_offset + 1].to_string(),
                id: sources[i][course_offset + 2].to_string(),
                grade: sources[i][course_offset + 6].to_string(),
            },
            school,
        });
        i += 1;
    }

    transfers
}

fn process_semesters(chunks: &[Chunk]) -> Vec<Semester> {
    fn get_year_term(s: &str) -> Option<(&str, &str)> {
        let mut pieces = s.split_ascii_whitespace();
        match (pieces.next(), pieces.next()) {
            (Some(year), Some(term)) if ["Spring", "Summer", "Fall"].contains(&term) => {
                Some((year, term))
            }
            _ => None,
        }
    }

    let grouped = chunks
        .chunk_by(|_, b| !matches!(b, Chunk::String(s) if get_year_term(s).is_some()))
        .skip(1)
        .filter(|s| s.len() >= 2)
        .map(|s| {
            (
                get_year_term(s[0].get_string().unwrap()).unwrap(),
                s[1..]
                    .iter()
                    // Rows are ragged, so map elements to strings and filter out
                    // conditional elements like qualifiers to make columns align.
                    .filter_map(|c| c.get_contained())
                    .map(|row| {
                        row.iter()
                            .filter_map(|c| c.get_string())
                            .filter(|s| !is_qualifier(s) && !matches_breadth(s) && !is_perm_dt(s))
                            .collect::<Vec<_>>()
                    })
                    // Exclude rows for GPA or courses without grades
                    .filter(|v| !v[0].ends_with("GPA:") && 6 < v.len() && !v[6].is_empty())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();

    grouped
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|((year, term), rows)| Semester {
            year: (*year).to_string(),
            term: (*term).to_string(),
            is_good_standing: true,
            courses: rows
                .iter()
                // Including asserting inspections helps to sanity check the
                // correctness of the extraction because of the reverse
                // engineered format.
                .inspect(|v| assert!(POSSIBLE_GRADES.contains(&v[6])))
                .map(|r| Course {
                    subject: r[1].to_string(),
                    id: r[2].to_string(),
                    grade: r[6].to_string(),
                })
                .collect(),
        })
        .collect::<Vec<_>>()
}

fn process_chunks(chunks: &[Chunk]) -> Result<StudentInfo, Error> {
    fn find_index(chunks: &[Chunk], start: usize, marker: &str, err: &str) -> Result<usize, Error> {
        let marker_chunk = Chunk::String(marker.to_string());
        Ok(start
            + chunks[start..]
                .iter()
                .position(|c| c == &marker_chunk)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, err))?)
    }

    let plan_marker = "Plan";
    let plan_index = 1 + find_index(chunks, 0, plan_marker, "Plan marker not found")?;

    // This section is optional, so errors are nonfatal
    let transfer_marker = "TRANSFER COURSES";
    let transfer_index = find_index(
        chunks,
        plan_index,
        transfer_marker,
        "Transfer marker not found",
    );

    let program_marker = "Program:";
    let program_index = find_index(
        chunks,
        plan_index,
        program_marker,
        "Program marker not found",
    )?;

    let end_marker = "TOTAL UNITS PASSED BY ACADEMIC GROUP";
    let end_index = find_index(chunks, program_index, end_marker, "End marker not found")?;

    let id_index = chunks.len() - 3;

    Ok(StudentInfo {
        id: chunks[id_index]
            .get_string()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Bad student id"))?
            .to_string(),
        plan: process_plan(&chunks[plan_index])?,
        transfers: transfer_index.map(|i| process_transfers(&chunks[i..program_index]))?,
        semesters: process_semesters(&chunks[program_index..end_index]),
    })
}

fn write_long_csv<W: std::io::Write>(writer: &mut csv::Writer<W>,
                                     student: &StudentInfo,
                                     new_id: usize) -> Result<(), Error> {
    for transfer in &student.transfers {
        writer.write_record([
            &new_id.to_string(),
            &student.plan.name,
            "None",
            "None",
            &transfer.course.subject,
            &transfer.course.id,
            &transfer.course.grade,
            transfer.school.as_ref()
                           .map(String::as_str)
                           .unwrap_or("None"),
        ])?;
    }
    for semester in &student.semesters {
        for course in &semester.courses {
            writer.write_record([
                &new_id.to_string(),
                &student.plan.name,
                &semester.year,
                &semester.term,
                &course.subject,
                &course.id,
                &course.grade,
                "",
            ])?;
        }
    }
    writer.flush()?;
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to input file
    #[arg(short, long)]
    input: std::path::PathBuf,

    /// Anonymized student ID to use during export
    #[arg(short, long)]
    newid: usize,    
}

fn main() -> Result<(), Error> {
    let args = Args::parse();

    match Document::load(args.input) {
        Ok(document) => {
            let chunks = extract_page_chunks(&document).unwrap();
            let simplified: Vec<Vec<Chunk>> = chunks
                .into_iter()
                .map(|page| page.into_iter().map(Chunk::simplify).collect())
                .collect();
            let combined = combine_page_chunks(simplified).unwrap();
            let student = process_chunks(&combined).unwrap();

            let mut writer = csv::Writer::from_writer(std::io::stdout());
            write_long_csv(&mut writer, &student, args.newid)?;
        }
        Err(err) => eprintln!("Error: {err}"),
    }
    Ok(())
}

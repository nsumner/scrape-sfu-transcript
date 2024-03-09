# scrape-sfu-transcript

... is a utility for taking SFU SIMS transcript PDFs, extracting the structured
information, and producing anonymized CSVs. The anonymized information is
suitable for post hoc or exploratory data analysis via other means.


## Getting Started

### Building

Right now, there is not a coordinated installation process.
Instead, you can use it by cloning the repo and building from source.
The tools is written in [Rust](https://www.rust-lang.org/), so you must first
[install Rust](https://rustup.rs/).

```bash
git clone git@github.com:nsumner/scrape-sfu-transcript.git
cd scrape-sfu-transcript/
cargo build --release
```


### Extracting from one PDF

From within the same build directory, you can run the tool using `cargo run`:

```bash
cargo run --release -- --pdf <path to SIMS PDF transcript> --newid <anonymized student id>
```

where the `--pdf` option specifies a single pdf file to extract. The anonymized
id is a numerical identifier that is not protected and not associated with the
student. You could choose the anonymized IDs for each student outside of the tool.

For example, if you have a SIMS transcript saved at the location
`~/teaching/sfusr-some-student-transcript.PDF` and want to give
that student the anonymized ID 42, you would run:

```bash
cargo run --release -- --pdf ~/teaching/sfusr-some-student-transcript.PDF --newid 42
```

Which will produce a "long" CSV including both transfer and SFU course
information in a form like:

```bash
42,CMPTMAJ,None,None,CMPT,130,B,UBC
42,CMPTMAJ,None,None,CMPT,135,TR,UBC
42,CMPTMAJ,2017,Summer,CMPT,225,B-,
42,CMPTMAJ,2017,Fall,CMPT,276,C+,
42,CMPTMAJ,2017,Fall,MACM,201,F,
42,CMPTMAJ,2018,Spring,CMPT,363,A-,
42,CMPTMAJ,2018,Spring,MACM,201,A,
42,CMPTMAJ,2018,Fall,CMPT,295,C-,
42,CMPTMAJ,2018,Fall,CMPT,310,C-,
42,CMPTMAJ,2018,Fall,CMPT,353,C-,
42,CMPTMAJ,2019,Spring,CMPT,300,B-,
42,CMPTMAJ,2019,Spring,CMPT,307,F,
42,CMPTMAJ,2019,Spring,CMPT,354,B+,
42,CMPTMAJ,2019,Spring,CMPT,376W,A,
42,CMPTMAJ,2019,Summer,CMPT,379,B+,
42,CMPTMAJ,2019,Summer,CMPT,383,B,
42,CMPTMAJ,2019,Fall,CMPT,272,A,
42,CMPTMAJ,2019,Fall,CMPT,373,WD,
42,CMPTMAJ,2020,Spring,CMPT,213,C,
42,CMPTMAJ,2020,Spring,CMPT,303,A,
42,CMPTMAJ,2020,Spring,CMPT,475,WD,
42,CMPTMAJ,2020,Summer,CMPT,454,A,
```
The first two rows show transferred credits and the source institution.
The following rows show courses taken at SFU. The structure of the "long" form
CSVs has the columns:

```bash
Student ID, Program, Year, Term, Subject, Course ID, Grade, Transfer Institution
```

where `Year` and `Term` only apply to SFU courses and `Transfer Institution`
only applies to credits transferred in.

### Extracting from a directory containing PDFs

Similarly, you can specify a directory and extract information from all PDFs in
that directory. From within the same build directory, you again run the tool
using `cargo run`:

```bash
cargo run --release -- --dir <path to directory of transcripts> --newid <first anonymized student id>
```

The `--dir` option specifies a directory of PDF transcripts. All PDFs in that
directory will be process (in random order) and given anonymized student IDs in
the range [`newid`, `newid` + #transcripts). If any PDF in that directory is
not an SFU SIMS transcript, the program will simply crash rather than proceed.

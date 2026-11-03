//! Prepared graph values shared by XMind and structured-data readers.
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MindMap {
    pub path: PathBuf,
    pub fingerprint: String,
    pub sheets: Vec<Sheet>,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sheet {
    pub title: String,
    pub topics: Vec<Topic>,
    pub roots: Vec<usize>,
    pub relationships: Vec<(usize, usize)>,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Topic {
    pub title: String,
    pub notes: String,
    pub labels: Vec<String>,
    pub link: Option<String>,
    pub children: Vec<usize>,
}
impl MindMap {
    pub fn heap_bytes(&self) -> u64 {
        self.fingerprint.capacity() as u64
            + self.path.as_os_str().len() as u64
            + (self.sheets.capacity() * std::mem::size_of::<Sheet>()) as u64
            + self
                .sheets
                .iter()
                .map(|sheet| {
                    sheet.title.capacity() as u64
                        + (sheet.topics.capacity() * std::mem::size_of::<Topic>()) as u64
                        + (sheet.roots.capacity() * std::mem::size_of::<usize>()) as u64
                        + (sheet.relationships.capacity() * std::mem::size_of::<(usize, usize)>())
                            as u64
                        + sheet
                            .topics
                            .iter()
                            .map(|topic| {
                                (topic.title.capacity() + topic.notes.capacity()) as u64
                                    + topic.link.as_ref().map_or(0, |link| link.capacity() as u64)
                                    + (topic.children.capacity() * std::mem::size_of::<usize>())
                                        as u64
                                    + (topic.labels.capacity() * std::mem::size_of::<String>())
                                        as u64
                                    + topic
                                        .labels
                                        .iter()
                                        .map(|label| label.capacity() as u64)
                                        .sum::<u64>()
                            })
                            .sum::<u64>()
                })
                .sum::<u64>()
    }
}

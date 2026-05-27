use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    AutomationScript,
    DataProcessing,
    Crawler,
    CvMl,
    FrontendWeb,
    DocumentPdf,
    ResearchWriting,
    Custom,
}

impl ProjectType {
    pub const ALL: &'static [ProjectType; 8] = &[
        ProjectType::AutomationScript,
        ProjectType::DataProcessing,
        ProjectType::Crawler,
        ProjectType::CvMl,
        ProjectType::FrontendWeb,
        ProjectType::DocumentPdf,
        ProjectType::ResearchWriting,
        ProjectType::Custom,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectType::AutomationScript => "automation_script",
            ProjectType::DataProcessing => "data_processing",
            ProjectType::Crawler => "crawler",
            ProjectType::CvMl => "cv_ml",
            ProjectType::FrontendWeb => "frontend_web",
            ProjectType::DocumentPdf => "document_pdf",
            ProjectType::ResearchWriting => "research_writing",
            ProjectType::Custom => "custom",
        }
    }
}

impl fmt::Display for ProjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProjectType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "automation_script" => ProjectType::AutomationScript,
            "data_processing" => ProjectType::DataProcessing,
            "crawler" => ProjectType::Crawler,
            "cv_ml" => ProjectType::CvMl,
            "frontend_web" => ProjectType::FrontendWeb,
            "document_pdf" => ProjectType::DocumentPdf,
            "research_writing" => ProjectType::ResearchWriting,
            "custom" => ProjectType::Custom,
            other => return Err(Error::Invalid(format!("unknown project type: {other}"))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_type_roundtrips_document_pdf() {
        let project_type = ProjectType::from_str("document_pdf").unwrap();

        assert_eq!(project_type.as_str(), "document_pdf");
        assert!(ProjectType::ALL
            .iter()
            .any(|candidate| candidate.as_str() == "document_pdf"));
    }
}

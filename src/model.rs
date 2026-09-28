use rusqlite::Row;

#[derive(Debug, Clone)]
pub struct TimeEntry {
    pub id: i64,
    pub work_date: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub task_id: Option<String>,
    pub task_name: Option<String>,
    pub hours: f64,
    pub description: Option<String>,
    pub remote_id: Option<String>,
    pub status: String,
    pub entry_status: Option<String>,
    pub error: Option<String>,
}

impl std::fmt::Display for TimeEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "#{} {} | {}h | {} | {}",
            self.id,
            self.work_date,
            self.hours,
            self.project_name.as_deref().unwrap_or("Ohne Projekt"),
            self.description.as_deref().unwrap_or_default()
        )
    }
}

impl TimeEntry {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            work_date: row.get("work_date")?,
            project_id: row.get("project_id")?,
            project_name: row.get("project_name")?,
            task_id: row.get("task_id")?,
            task_name: row.get("task_name")?,
            hours: row.get("hours")?,
            description: row.get("description")?,
            remote_id: row.get("remote_id")?,
            status: row.get("status")?,
            entry_status: row.get("entry_status")?,
            error: row.get("error")?,
        })
    }
}

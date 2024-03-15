mod macros;
mod render;

use core::fmt::Arguments;
use std::{
    error::Error,
    fs::File,
    io,
    io::{
        Read,
        Write,
    },
    path::PathBuf,
};

use chrono::{
    Datelike,
    Duration,
    NaiveDate,
    Weekday,
};
use clap::Parser;
use easy_error::{
    bail,
    ResultExt,
};
use rand::Rng;
use serde::{
    Deserialize,
    Serialize,
};
use svg::{
    node::{
        element::{
            path::Data,
            Group,
            Line,
            Path,
            Rectangle,
            Style,
            Text,
        },
        Blob,
    },
    Document,
    Node,
};

static GOLDEN_RATIO_CONJUGATE: f32 = 0.618034; // 0.618033988749895
static MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Parser)]
#[clap(version, about, long_about = None)]
struct Cli {
    /// Specify the JSON data file
    #[arg(value_name = "INPUT_FILE")]
    input_file: Option<PathBuf>,

    /// The SVG output file
    #[arg(value_name = "OUTPUT_FILE")]
    output_file: Option<PathBuf>,

    /// The width of the item title column
    #[arg(value_name = "WIDTH", short, long, default_value_t = 210.0)]
    title_width: f32,

    /// The maximum width of each month
    #[arg(value_name = "WIDTH", short, long, default_value_t = 200.0)]
    max_month_width: f32,

    /// Add a resource table at the bottom of the graph
    #[arg(short, long, default_value_t = false)]
    legend: bool,
}

impl Cli {
    fn get_output(&self) -> Result<Box<dyn Write>, Box<dyn Error>> {
        match self.output_file {
            Some(ref path) => File::create(path)
                .context(format!(
                    "Unable to create file '{}'",
                    path.to_string_lossy()
                ))
                .map(|f| Box::new(f) as Box<dyn Write>)
                .map_err(|e| Box::new(e) as Box<dyn Error>),
            None => Ok(Box::new(io::stdout())),
        }
    }

    fn get_input(&self) -> Result<Box<dyn Read>, Box<dyn Error>> {
        match self.input_file {
            Some(ref path) => File::open(path)
                .context(format!("Unable to open file '{}'", path.to_string_lossy()))
                .map(|f| Box::new(f) as Box<dyn Read>)
                .map_err(|e| Box::new(e) as Box<dyn Error>),
            None => Ok(Box::new(io::stdin())),
        }
    }
}

pub trait GanttChartLog {
    fn output(&self, args: Arguments);
    fn warning(&self, args: Arguments);
    fn error(&self, args: Arguments);
}

pub struct GanttChartTool<'a> {
    log: &'a dyn GanttChartLog,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ItemData {
    pub title: String,
    pub duration: Option<i64>,
    #[serde(rename = "startDate", skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(rename = "resource")]
    pub resource_index: Option<usize>,
    pub open: Option<bool>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ChartData {
    pub title: String,
    #[serde(rename = "markedDate")]
    pub marked_date: Option<NaiveDate>,
    pub resources: Vec<String>,
    pub items: Vec<ItemData>,
}

#[derive(Debug)]
pub struct Gutter {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Gutter {
    pub fn height(&self) -> f32 {
        self.bottom + self.top
    }

    pub fn width(&self) -> f32 {
        self.right + self.left
    }
}

#[derive(Debug)]
struct RenderData {
    title: String,
    gutter: Gutter,
    row_gutter: Gutter,
    row_height: f32,
    resource_gutter: Gutter,
    resource_height: f32,
    marked_date_offset: Option<f32>,
    title_width: f32,
    max_month_width: f32,
    rect_corner_radius: f32,
    styles: Vec<String>,
    cols: Vec<ColumnRenderData>,
    rows: Vec<RowRenderData>,
    resources: Vec<String>,
}

#[derive(Debug)]
struct RowRenderData {
    title: String,
    resource_index: usize,
    offset: f32,
    // If length not present then this is a milestone
    length: Option<f32>,
    open: bool,
}

#[derive(Debug)]
struct ColumnRenderData {
    width: f32,
    month_name: String,
}

impl<'a> GanttChartTool<'a> {
    pub fn new(log: &'a dyn GanttChartLog) -> GanttChartTool {
        GanttChartTool { log }
    }

    pub fn run(
        &mut self,
        args: impl IntoIterator<Item = std::ffi::OsString>,
    ) -> Result<(), Box<dyn Error>> {
        let cli = match Cli::try_parse_from(args) {
            Ok(cli) => cli,
            Err(err) => {
                output!(self.log, "{}", err.to_string());
                return Ok(());
            }
        };

        let chart_data = Self::read_chart_file(cli.get_input()?)?;
        let render_data =
            self.process_chart_data(cli.title_width, cli.max_month_width, &chart_data)?;
        let output = self.render_chart(cli.legend, &render_data)?;

        Self::write_svg_file(cli.get_output()?, &output)?;
        Ok(())
    }

    fn read_chart_file(mut reader: Box<dyn Read>) -> Result<ChartData, Box<dyn Error>> {
        let mut content = String::new();

        reader.read_to_string(&mut content)?;

        let chart_data: ChartData = json5::from_str(&content)?;

        Ok(chart_data)
    }

    fn write_svg_file(mut writer: Box<dyn Write>, output: &str) -> Result<(), Box<dyn Error>> {
        write!(writer, "{}", output)?;

        Ok(())
    }

    fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
        let h_i = (h * 6.0) as usize;
        let f = h * 6.0 - h_i as f32;
        let p = v * (1.0 - s);
        let q = v * (1.0 - f * s);
        let t = v * (1.0 - (1.0 - f) * s);

        fn rgb(r: f32, g: f32, b: f32) -> u32 {
            ((r * 256.0) as u32) << 16 | ((g * 256.0) as u32) << 8 | ((b * 256.0) as u32)
        }

        if h_i == 0 {
            rgb(v, t, p)
        } else if h_i == 1 {
            rgb(q, v, p)
        } else if h_i == 2 {
            rgb(p, v, t)
        } else if h_i == 3 {
            rgb(p, q, v)
        } else if h_i == 4 {
            rgb(t, p, v)
        } else {
            rgb(v, p, q)
        }
    }

    fn process_chart_data(
        &self,
        title_width: f32,
        max_month_width: f32,
        chart_data: &ChartData,
    ) -> Result<RenderData, Box<dyn Error>> {
        fn num_days_in_month(year: i32, month: u32) -> u32 {
            // the first day of the next month...
            let (y, m) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            let d = NaiveDate::from_ymd_opt(y, m, 1).unwrap(); // FIXME unwrap

            // ...is preceded by the last day of the original month
            d.pred_opt().unwrap().day() // FIXME unwrap
        }

        // Fail if only one task
        if chart_data.items.len() < 2 {
            bail!("You must provide more than one task");
        }

        let mut start_date = NaiveDate::MAX;
        let mut end_date = NaiveDate::MIN;
        let mut date = NaiveDate::MIN;
        let mut shadow_durations: Vec<Option<i64>> = Vec::with_capacity(chart_data.items.len());

        // Determine the project start & end dates
        for (i, item) in chart_data.items.iter().enumerate() {
            if let Some(item_start_date) = item.start_date {
                date = item_start_date;

                if item_start_date < start_date {
                    // Move the start if it falls on a weekend
                    start_date = match date.weekday() {
                        Weekday::Sat => date + Duration::try_days(2).unwrap(), // FIXME unwrap
                        Weekday::Sun => date + Duration::try_days(1).unwrap(), // FIXME unwrap
                        _ => date,
                    };
                }
            } else if i == 0 {
                return Err(From::from(
                    "First item must contain a start date".to_string(),
                ));
            }

            // Skip the weekends and update a shadow list of the _real_ durations
            if let Some(item_days) = item.duration {
                // FIXME unwrap
                let duration = match (date + Duration::try_days(item_days).unwrap()).weekday() {
                    Weekday::Sat => Duration::try_days(item_days + 2).unwrap(),
                    Weekday::Sun => Duration::try_days(item_days + 1).unwrap(),
                    _ => Duration::try_days(item_days).unwrap(),
                };

                date += duration;

                shadow_durations.push(Some(duration.num_days()));
            } else {
                shadow_durations.push(None);
            }

            if end_date < date {
                end_date = date;
            }

            if let Some(item_resource_index) = item.resource_index {
                if item_resource_index >= chart_data.resources.len() {
                    return Err(From::from("Resource index is out of range".to_string()));
                }
            } else if i == 0 {
                return Err(From::from(
                    "First item must contain a resource index".to_string(),
                ));
            }
        }

        start_date = NaiveDate::from_ymd_opt(start_date.year(), start_date.month(), 1).unwrap(); // FIXME unwrap
        end_date = NaiveDate::from_ymd_opt(
            end_date.year(),
            end_date.month(),
            num_days_in_month(end_date.year(), end_date.month()),
        )
        .unwrap(); // FIXME unwrap

        // Create all the column data
        let mut all_items_width: f32 = 0.0;
        let mut num_item_days: u32 = 0;
        let mut cols = vec![];

        date = start_date;

        while date <= end_date {
            let item_days = num_days_in_month(date.year(), date.month());
            let item_width = max_month_width * (item_days as f32) / 31.0;

            num_item_days += item_days;
            all_items_width += item_width;

            cols.push(ColumnRenderData {
                width: item_width,
                month_name: MONTH_NAMES[date.month() as usize - 1].to_string(),
            });

            date = NaiveDate::from_ymd_opt(
                date.year() + (if date.month() == 12 { 1 } else { 0 }),
                date.month() % 12 + 1,
                1,
            )
            .unwrap(); // FIXME unwrap
        }

        date = start_date;

        let mut resource_index: usize = 0;
        let gutter = Gutter {
            left: 10.0,
            top: 80.0,
            right: 10.0,
            bottom: 10.0,
        };
        let row_gutter = Gutter {
            left: 5.0,
            top: 5.0,
            right: 5.0,
            bottom: 5.0,
        };
        // TODO(john): The 20.0 should be configurable, and for the resource table
        let row_height = row_gutter.height() + 20.0;
        let resource_gutter = Gutter {
            left: 10.0,
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
        };
        let resource_height = resource_gutter.height() + 20.0;
        let mut rows = vec![];

        // Calculate the X offsets of all the bars and milestones
        for (i, item) in chart_data.items.iter().enumerate() {
            if let Some(item_start_date) = item.start_date {
                date = item_start_date;
            }

            let offset = title_width
                + gutter.left
                + ((date - start_date).num_days() as f32) / (num_item_days as f32)
                    * all_items_width;

            let mut length: Option<f32> = None;

            if let Some(item_days) = shadow_durations[i] {
                // Use the shadow duration instead of the actual duration as it accounts for weekends
                date += Duration::try_days(item_days).unwrap(); // FIXME unwrap
                length = Some((item_days as f32) / (num_item_days as f32) * all_items_width);
            }

            if let Some(item_resource_index) = item.resource_index {
                resource_index = item_resource_index;
            }

            rows.push(RowRenderData {
                title: item.title.clone(),
                resource_index,
                offset,
                length,
                open: item.open.unwrap_or(false),
            });
        }

        let marked_date_offset = chart_data.marked_date.map(|date| {
            title_width
                + gutter.left
                + ((date - start_date).num_days() as f32) / (num_item_days as f32) * all_items_width
        });

        let mut styles: Vec<String> = vec_of_strings![
            ".outer-lines{ stroke-width:3; stroke:#aaaaaa;}",
            ".inner-lines{ stroke-width:2; stroke:#dddddd;}",
            ".item{font-family:Arial; font-size:12pt; dominant-baseline:middle;}",
            ".resource{font-family:Arial; font-size:12pt; text-anchor:end; dominant-baseline:middle;}",
            ".title{font-family:Arial; font-size:18pt;}",
            ".heading{font-family:Arial; font-size:16pt; dominant-baseline:middle; text-anchor:middle;}",
            ".task-heading{dominant-baseline:middle; text-anchor:start;}",
            ".milestone{fill:black;stroke-width:1;stroke:black;}",
            ".marker{stroke-width:2; stroke:#888888; stroke-dasharray:7;}"
        ];

        // Generate random resource colors based on https://martin.ankerl.com/2009/12/09/how-to-create-random-colors-programmatically/
        let mut rng = rand::thread_rng();
        let mut h: f32 = rng.gen();

        for i in 0..chart_data.resources.len() {
            let rgb = GanttChartTool::hsv_to_rgb(h, 0.5, 0.5);

            styles.push(format!(
                ".resource-{i}-closed{{stroke-width:1; stroke:#{rgb:06x}; fill:#{rgb:06x};}}"
            ));
            styles.push(format!(
                ".resource-{i}-open{{stroke-width:2; stroke:#{rgb:06x}; fill:none;}}"
            ));

            h = (h + GOLDEN_RATIO_CONJUGATE) % 1.0;
        }

        Ok(RenderData {
            title: chart_data.title.to_owned(),
            gutter,
            row_gutter,
            row_height,
            resource_gutter,
            resource_height,
            styles,
            title_width,
            max_month_width,
            marked_date_offset,
            rect_corner_radius: 3.0,
            cols,
            rows,
            resources: chart_data.resources.clone(),
        })
    }

    fn render_chart(&self, use_legend: bool, chart: &RenderData) -> Result<String, Box<dyn Error>> {
        let width: f32 = chart.gutter.left
            + chart.title_width
            + chart.cols.iter().map(|col| col.width).sum::<f32>()
            + chart.gutter.right;
        let height = chart.gutter.top
            + (chart.rows.len() as f32 * chart.row_height)
            + (if use_legend {
                chart.resource_gutter.height() + chart.row_height
            } else {
                0.0
            })
            + chart.gutter.bottom;

        let mut doc = Document::new()
            .set("width", width)
            .set("height", height)
            .set("viewBox", (0, 0, width, height))
            .set("style", "background-color: white;");

        let mut style = Style::new("");
        for s in chart.styles.iter() {
            style.append(Blob::new(s));
        }

        doc.append(style);

        // Render rows
        let mut rows_g = Group::new();
        let x1 = chart.gutter.left;
        let x2 = width - chart.gutter.right;
        for (i, row) in chart.rows.iter().enumerate() {
            let y = chart.gutter.top + (i as f32 * chart.row_height);
            let line_class = if i == 0 { "outer-lines" } else { "inner-lines" };

            rows_g.append(
                Text::new(&row.title)
                    .set("class", "item")
                    .set("x", chart.gutter.left + chart.row_gutter.left)
                    .set("y", y + chart.row_gutter.top + chart.row_height / 2.0),
            );

            // Is this a task or a milestone?
            if let Some(length) = row.length {
                // task
                let bar_class = format!(
                    "resource-{}{}",
                    row.resource_index,
                    if row.open { "-open" } else { "-closed" }
                );
                rows_g.append(
                    Rectangle::new()
                        .set("class", bar_class)
                        .set("x", row.offset)
                        .set("y", y + chart.row_gutter.top)
                        .set("rx", chart.rect_corner_radius)
                        .set("ry", chart.rect_corner_radius)
                        .set("width", length)
                        .set("height", chart.row_height - chart.row_gutter.height()),
                );
            } else {
                // milestone
                let n = (chart.row_height - chart.row_gutter.height()) / 2.0;
                rows_g.append(
                    Path::new().set(
                        "d",
                        Data::new()
                            .move_to((row.offset - n, y + chart.row_gutter.top + n))
                            .line_by((n, -n))
                            .line_by((n, n))
                            .line_by((-n, n))
                            .line_by((-n, -n))
                            .close(),
                    ),
                );
            }

            rows_g.append(
                Line::new()
                    .set("class", line_class)
                    .set("x1", x1)
                    .set("y1", y)
                    .set("x2", x2)
                    .set("y2", y),
            );
        }
        // last row
        {
            let y = chart.gutter.top + (chart.rows.len() as f32 * chart.row_height);
            rows_g.append(
                Line::new()
                    .set("class", "outer-lines")
                    .set("x1", x1)
                    .set("y1", y)
                    .set("x2", x2)
                    .set("y2", y),
            );
        }

        doc.append(rows_g);

        // Render columns
        let mut cols_g = Group::new();
        let y2 = chart.gutter.top + ((chart.rows.len() as f32) * chart.row_height);
        for (i, col) in chart.cols.iter().enumerate() {
            let line_x = chart.gutter.left
                + chart.title_width
                + chart.cols.iter().take(i).map(|col| col.width).sum::<f32>();
            let name_y = chart.gutter.top - chart.row_gutter.bottom - chart.row_height / 2.0;

            cols_g.append(
                Text::new(&col.month_name)
                    .set("class", "heading")
                    .set("x", line_x + chart.max_month_width / 2.0)
                    .set("y", name_y),
            );

            cols_g.append(
                Line::new()
                    .set("class", "inner-lines")
                    .set("x1", line_x)
                    .set("y1", chart.gutter.top)
                    .set("x2", line_x)
                    .set("y2", y2),
            );
        }
        // last line
        {
            let x = chart.gutter.left + chart.title_width;
            cols_g.append(
                Line::new()
                    .set("class", "inner-lines")
                    .set("x1", x)
                    .set("y1", chart.gutter.top)
                    .set("x2", x)
                    .set("y2", y2),
            );
        }

        doc.append(cols_g);

        // "Tasks" header
        {
            let x = chart.gutter.left + chart.row_gutter.left;
            let y = chart.gutter.top - chart.row_gutter.bottom - chart.row_height / 2.0;
            doc.append(
                Text::new("Tasks")
                    .set("class", "heading task-heading")
                    .set("x", x)
                    .set("y", y),
            );
        }

        // Chart title
        {
            doc.append(
                Text::new(&chart.title)
                    .set("class", "title")
                    .set("x", chart.gutter.left)
                    .set("y", 25.0),
            );
        }

        // Date marker
        {
            if let Some(offset) = chart.marked_date_offset {
                let y1 = chart.gutter.top - 5.0;
                let y2 = chart.gutter.top + ((chart.rows.len() as f32) * chart.row_height) + 5.0;
                doc.append(
                    Line::new()
                        .set("class", "marker")
                        .set("x1", offset)
                        .set("y1", y1)
                        .set("x2", offset)
                        .set("y2", y2),
                );
            }
        }

        // Legend
        if use_legend {
            let mut legend_g = Group::new();
            for (i, res) in chart.resources.iter().enumerate() {
                let y = chart.gutter.top + ((chart.rows.len() as f32) * chart.row_height);
                let block_width = chart.resource_height - chart.resource_gutter.height();

                let res_x = chart.resource_gutter.left + ((i + 1) as f32) * 100.0 - 5.0;
                let res_y = y + chart.resource_height / 2.0;
                legend_g.append(
                    Text::new(res)
                        .set("class", "resource")
                        .set("x", res_x)
                        .set("y", res_y),
                );

                let rect_x = chart.resource_gutter.left + ((i + 1) as f32) * 100.0 + 5.0;
                let rect_y = y + chart.resource_gutter.top;
                legend_g.append(
                    Rectangle::new()
                        .set("class", format!("resource-{}-closed", i))
                        .set("x", rect_x)
                        .set("y", rect_y)
                        .set("rx", chart.rect_corner_radius)
                        .set("ry", chart.rect_corner_radius)
                        .set("width", block_width)
                        .set("height", block_width),
                );
            }

            doc.append(legend_g);
        }

        Ok(doc.to_string())
    }
}

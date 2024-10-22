#![allow(unused)]
use anyhow::Result;
use client::proto::ViewId;
use gpui::{
    actions, prelude::*, AppContext, EventEmitter, FocusHandle, FocusableView, Model, Task, View,
    WeakView,
};
use project::Project;
use ui::prelude::*;
use util::ResultExt;
use workspace::{FollowableItem, Item, ItemHandle, Pane, Workspace};

actions!(
    notebook,
    [
        OpenNotebook,
        RunAll,
        ClearOutputs,
        MoveCellUp,
        MoveCellDown,
        AddMarkdownBlock,
        AddCodeBlock
    ]
);

const MAX_TEXT_BLOCK_WIDTH: f32 = 9999.0;
const SMALL_SPACING_SIZE: f32 = 8.0;
const MEDIUM_SPACING_SIZE: f32 = 12.0;
const LARGE_SPACING_SIZE: f32 = 16.0;
const GUTTER_WIDTH: f32 = 19.0;
const CODE_BLOCK_INSET: f32 = MEDIUM_SPACING_SIZE;
const CONTROL_SIZE: f32 = 20.0;

const DEFAULT_NOTEBOOK_FORMAT: i32 = 4;
const DEFAULT_NOTEBOOK_FORMAT_MINOR: i32 = 0;

pub fn init(cx: &mut AppContext) {
    cx.observe_new_views(|workspace: &mut Workspace, _| {
        workspace.register_action(|_, _: &OpenNotebook, cx| {
            let workspace = cx.view().clone();
            cx.window_context()
                .defer(move |cx| Notebook::open(workspace, cx).detach_and_log_err(cx));
        });
    })
    .detach();
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NotebookData {
    metadata: DeserializedMetadata,
    nbformat: i32,
    nbformat_minor: i32,
    cells: Vec<DeserializedCell>,
}

impl NotebookData {}

impl Default for NotebookData {
    fn default() -> Self {
        Self {
            metadata: Default::default(),
            nbformat: DEFAULT_NOTEBOOK_FORMAT,
            nbformat_minor: DEFAULT_NOTEBOOK_FORMAT_MINOR,
            cells: vec![],
        }
    }
}

impl Default for DeserializedMetadata {
    fn default() -> Self {
        Self {
            kernelspec: None,
            language_info: None,
        }
    }
}

pub struct Notebook {
    focus_handle: FocusHandle,
    workspace: WeakView<Workspace>,
    project: Model<Project>,
    remote_id: Option<ViewId>,
    selected_cell: usize,
    cells: Vec<Cell>,
    data: Model<NotebookData>,
}

impl Notebook {
    pub fn open(
        workspace_view: View<Workspace>,
        cx: &mut WindowContext,
    ) -> Task<Result<View<Self>>> {
        let weak_workspace = workspace_view.downgrade();
        let workspace = workspace_view.read(cx);
        let project = workspace.project().to_owned();
        let pane = workspace.active_pane().clone();
        let notebook = Self::load(workspace_view, cx);

        cx.spawn(|mut cx| async move {
            let notebook = notebook.await?;
            pane.update(&mut cx, |pane, cx| {
                pane.add_item(Box::new(notebook.clone()), true, true, None, cx);
            })?;

            anyhow::Ok(notebook)
        })
    }

    pub fn load(workspace: View<Workspace>, cx: &mut WindowContext) -> Task<Result<View<Self>>> {
        let weak_workspace = workspace.downgrade();
        let workspace = workspace.read(cx);
        let project = workspace.project().to_owned();

        cx.spawn(|mut cx| async move {
            cx.new_view(|cx| Self::new(weak_workspace.clone(), project, cx))
        })
    }

    pub fn new(
        workspace: WeakView<Workspace>,
        project: Model<Project>,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let this = cx.view().downgrade();
        let focus_handle = cx.focus_handle();
        let data = cx.new_model(|_| NotebookData::default());

        let cells = sample_cells();

        Self {
            focus_handle,
            workspace,
            project,
            remote_id: None,
            selected_cell: 0,
            cells,
            data,
        }
    }

    fn open_notebook(&mut self, _: &OpenNotebook, _cx: &mut ViewContext<Self>) {
        println!("Open notebook triggered");
    }

    fn button_group(cx: &ViewContext<Self>) -> Div {
        v_flex()
            .gap(Spacing::Small.rems(cx))
            .items_center()
            .w(px(CONTROL_SIZE + 4.0))
            .overflow_hidden()
            .rounded(px(5.))
            .bg(cx.theme().colors().title_bar_background)
            .p_px()
            .border_1()
            .border_color(cx.theme().colors().border)
    }

    fn render_control(
        id: impl Into<SharedString>,
        icon: IconName,
        cx: &ViewContext<Self>,
    ) -> IconButton {
        let id: ElementId = ElementId::Name(id.into());
        IconButton::new(id, icon).width(px(CONTROL_SIZE).into())
    }

    fn render_controls(cx: &ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .max_w(px(CONTROL_SIZE + 4.0))
            .items_center()
            .gap(Spacing::XXLarge.rems(cx))
            .justify_between()
            .flex_none()
            .h_full()
            .child(
                v_flex()
                    .gap(Spacing::Large.rems(cx))
                    .child(
                        Self::button_group(cx)
                            .child(Self::render_control("run-all-cells", IconName::Play, cx))
                            .child(Self::render_control(
                                "clear-all-outputs",
                                IconName::Close,
                                cx,
                            )),
                    )
                    .child(
                        Self::button_group(cx)
                            .child(
                                Self::render_control("move-cell-up", IconName::ChevronUp, cx)
                                    .disabled(true),
                            )
                            .child(Self::render_control(
                                "move-cell-down",
                                IconName::ChevronDown,
                                cx,
                            )),
                    )
                    .child(
                        Self::button_group(cx)
                            .child(Self::render_control(
                                "new-markdown-cell",
                                IconName::Plus,
                                cx,
                            ))
                            .child(Self::render_control("new-code-cell", IconName::Code, cx)),
                    ),
            )
            .child(
                v_flex()
                    .gap(Spacing::Large.rems(cx))
                    .items_center()
                    .child(Self::render_control("more-menu", IconName::Ellipsis, cx))
                    .child(
                        Self::button_group(cx)
                            .child(IconButton::new("repl", IconName::ReplNeutral)),
                    ),
            )
    }
}

impl Render for Notebook {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        // cell bar
        // scrollbar
        // settings

        let large_gap = Spacing::XLarge.px(cx);
        let gap = Spacing::Large.px(cx);

        div()
            // .debug_below()
            .key_context("notebook")
            .on_action(cx.listener(Self::open_notebook))
            .track_focus(&self.focus_handle)
            .flex()
            .items_start()
            .size_full()
            .overflow_hidden()
            .p(large_gap)
            .gap(large_gap)
            .bg(cx.theme().colors().tab_bar_background)
            .child(Self::render_controls(cx))
            .child(
                // notebook cells
                v_flex()
                    .id("notebook-cells")
                    .flex_1()
                    .size_full()
                    .overflow_hidden()
                    .gap_6()
                    .children(self.cells.iter().enumerate().map(|(ix, cell)| {
                        let mut c = cell.clone(); // Clone the Cell while iterating
                        c.selected(self.selected_cell == ix) // Set the selected state
                    })),
            )
            .child(
                div()
                    .w(px(GUTTER_WIDTH))
                    .h_full()
                    .flex_none()
                    .overflow_hidden()
                    .child("cell bar")
                    .child("scrollbar"),
            )

        // .child("settings")
    }
}

impl FocusableView for Notebook {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for Notebook {}

impl Item for Notebook {
    type Event = ();

    fn tab_content_text(&self, _cx: &WindowContext) -> Option<SharedString> {
        // TODO: We want file name
        Some("Notebook".into())
    }

    fn tab_icon(&self, _cx: &ui::WindowContext) -> Option<Icon> {
        Some(IconName::Book.into())
    }

    fn show_toolbar(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
enum CellType {
    Code,
    Markdown,
    Raw,
}

#[derive(IntoElement, Clone)]
struct Cell {
    cell_type: CellType,
    control: Option<IconName>,
    source: Vec<String>,
    selected: bool,
}

impl RenderOnce for Cell {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let source = self.source.clone();
        let cell_type = self.cell_type.clone();
        let is_selected = self.selected.clone();
        let mut selected_bg = cx.theme().colors().icon_accent;
        selected_bg.fade_out(0.9);

        h_flex()
            .w_full()
            .items_start()
            .gap(Spacing::Large.rems(cx))
            .when(is_selected, |this| this.bg(selected_bg))
            .when(!is_selected, |this| {
                this.bg(cx.theme().colors().tab_bar_background)
            })
            .child(
                div()
                    .relative()
                    .h_full()
                    .w(px(GUTTER_WIDTH))
                    .child(
                        div()
                            .w(px(GUTTER_WIDTH))
                            .flex()
                            .flex_none()
                            .justify_center()
                            .h_full()
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(1.))
                                    .h_full()
                                    .when(is_selected, |this| {
                                        this.bg(cx.theme().colors().icon_accent)
                                    })
                                    .when(!is_selected, |this| this.bg(cx.theme().colors().border)),
                            ),
                    )
                    .children(self.control.map(|action| {
                        div()
                            .absolute()
                            .top(px(CODE_BLOCK_INSET - 2.0))
                            .left_0()
                            .flex()
                            .flex_none()
                            .w(px(GUTTER_WIDTH))
                            .h(px(GUTTER_WIDTH + 12.0))
                            .items_center()
                            .justify_center()
                            .bg(cx.theme().colors().tab_bar_background)
                            .child(IconButton::new("control", action))
                    })),
            )
            .when(cell_type == CellType::Markdown, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .max_w(px(MAX_TEXT_BLOCK_WIDTH))
                        .px(px(CODE_BLOCK_INSET))
                        .children(source.clone()),
                )
            })
            .when(cell_type == CellType::Code, |this| {
                this.child(
                    v_flex()
                        .size_full()
                        .flex_1()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().colors().border)
                        .bg(cx.theme().colors().editor_background)
                        .font_buffer(cx)
                        .text_size(TextSize::Editor.rems(cx))
                        .children(source),
                )
            })
    }
}

impl Cell {
    pub fn markdown(source: Vec<String>) -> Self {
        Self {
            control: None,
            cell_type: CellType::Markdown,
            source,
            selected: false,
        }
    }

    pub fn code(source: Vec<String>) -> Self {
        Self {
            control: None,
            cell_type: CellType::Code,
            source,
            selected: false,
        }
    }

    pub fn kind(mut self, kind: CellType) -> Self {
        self.cell_type = kind;
        self
    }

    pub fn control(mut self, control: IconName) -> Self {
        self.control = Some(control);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

// impl FollowableItem for Notebook {}

enum NotebookCell {
    Code(NotebookCodeCell),
    Markdown(NotebookMarkdownCell),
}

#[derive(IntoElement)]
struct NotebookCodeCell {}

impl NotebookCodeCell {
    fn new() -> Self {
        Self {}
    }
}

impl RenderOnce for NotebookCodeCell {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        h_flex()
            .w_full()
            .h(px(280.))
            .items_start()
            .gap(Spacing::Large.rems(cx))
            .child(
                div()
                    .relative()
                    .h_full()
                    .w(px(GUTTER_WIDTH))
                    .child(
                        div()
                            .w(px(GUTTER_WIDTH))
                            .flex()
                            .flex_none()
                            .justify_center()
                            .h_full()
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(1.))
                                    .h_full()
                                    .bg(cx.theme().colors().border),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(CODE_BLOCK_INSET - 2.0))
                            .left_0()
                            .flex()
                            .flex_none()
                            .w(px(GUTTER_WIDTH))
                            .h(px(GUTTER_WIDTH + 12.0))
                            .items_center()
                            .justify_center()
                            .bg(cx.theme().colors().tab_bar_background)
                            .child(IconButton::new("run", IconName::Play)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .size_full()
                    .flex_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().colors().border)
                    .bg(cx.theme().colors().editor_background)
                    .font_buffer(cx)
                    .text_size(TextSize::Editor.rems(cx))
                    .child("Code cell"),
            )
    }
}

#[derive(IntoElement)]
struct NotebookMarkdownCell {}

impl NotebookMarkdownCell {
    fn new() -> Self {
        Self {}
    }
}

impl RenderOnce for NotebookMarkdownCell {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_start()
            .gap(Spacing::Large.rems(cx))
            .child(
                div()
                    .w(px(GUTTER_WIDTH))
                    .flex()
                    .flex_none()
                    .justify_center()
                    .h_full()
                    .child(
                        div()
                            .flex_none()
                            .w(px(1.))
                            .h_full()
                            .bg(cx.theme().colors().border),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(MAX_TEXT_BLOCK_WIDTH))
                    .px(px(CODE_BLOCK_INSET))
                    .child(Headline::new("Population Data from CSV").size(HeadlineSize::Large))
                    .child("This notebook reads sample population data from `data/atlantis.csv` and plots it using matplotlib. Edit `data/atlantis.csv` and re-run this cell to see how the plots change!"),
            )
    }
}

fn sample_cells() -> Vec<Cell> {
    vec![
        Cell::markdown(vec![
            "Table of Contents".to_string(),
            "1.\tIntroduction".to_string(),
            "2.\tOverview of Python Data Visualization Tools".to_string(),
            "3.\tIntroduction to Matplotlib".to_string(),
            "4.\tImport Matplotlib".to_string(),
            "5.\tDisplaying Plots in Matplotlib".to_string(),
            "6.\tMatplotlib Object Hierarchy".to_string(),
            "7.\tMatplotlib interfaces".to_string(),
        ]),
        Cell::markdown(vec![
            "## 1. Introduction".to_string(),
            "When we want to convey some information to others, there are several ways to do so. The process of conveying the information with the help of plots and graphics is called **Data Visualization**. The plots and graphics take numerical data as input and display output in the form of charts, figures and tables. It helps to analyze and visualize the data clearly and make concrete decisions. It makes complex data more accessible and understandable. The goal of data visualization is to communicate information in a clear and efficient manner.".to_string(),
            "In this project, I shed some light on **Matplotlib**, which is the basic data visualization tool of Python programming language. Python has different data visualization tools available which are suitable for different purposes. First of all, I will list these data visualization tools and then I will discuss Matplotlib.".to_string()
        ])
    ]
}

trait RenderableCell: Render {
    fn cell_type(&self) -> CellType;
    fn metadata(&self) -> &DeserializedCellMetadata;
    fn source(&self) -> String;
}

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct DeserializedNotebook {
    metadata: DeserializedMetadata,
    nbformat: i32,
    nbformat_minor: i32,
    cells: Vec<DeserializedCell>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeserializedMetadata {
    kernelspec: Option<DeserializedKernelSpec>,
    language_info: Option<DeserializedLanguageInfo>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeserializedKernelSpec {
    name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeserializedLanguageInfo {
    name: String,
    version: Option<String>,
    codemirror_mode: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cell_type")]
pub enum DeserializedCell {
    #[serde(rename = "markdown")]
    Markdown {
        metadata: DeserializedCellMetadata,
        source: String,
        #[serde(default)]
        attachments: Option<serde_json::Value>,
    },
    #[serde(rename = "code")]
    Code {
        metadata: DeserializedCellMetadata,
        execution_count: Option<i32>,
        source: String,
        outputs: Vec<DeserializedOutput>,
    },
    #[serde(rename = "raw")]
    Raw {
        metadata: DeserializedCellMetadata,
        source: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeserializedCellMetadata {
    collapsed: Option<bool>,
    scrolled: Option<serde_json::Value>,
    deletable: Option<bool>,
    editable: Option<bool>,
    format: Option<String>,
    name: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "output_type")]
pub enum DeserializedOutput {
    #[serde(rename = "stream")]
    Stream { name: String, text: String },
    #[serde(rename = "display_data")]
    DisplayData {
        data: serde_json::Value,
        metadata: serde_json::Value,
    },
    #[serde(rename = "execute_result")]
    ExecuteResult {
        execution_count: i32,
        data: serde_json::Value,
        metadata: serde_json::Value,
    },
    #[serde(rename = "error")]
    Error {
        ename: String,
        evalue: String,
        traceback: Vec<String>,
    },
}

fn deserialize_notebook(notebook: &str) -> Result<DeserializedNotebook, serde_json::Error> {
    serde_json::from_str(notebook)
}

fn deserializable_sample_notebook() -> &'static str {
    r#"{
     "cells": [
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "Data Visualization with Matplotlib\n===",
        "\n",
        "\n",
        "This project is all about Matplotlib, the basic data visualization tool of Python programming language. I have discussed Matplotlib object hierarchy, various plot types with Matplotlib and customization techniques associated with Matplotlib. \n",
        "\n",
        "\n",
        "This project is divided into various sections based on contents which are listed below:- \n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "Table of Contents\n===",
        "\n",
        "\n",
        "1.\tIntroduction\n",
        "\n",
        "2.\tOverview of Python Data Visualization Tools\n",
        "\n",
        "3.\tIntroduction to Matplotlib\n",
        "\n",
        "4.\tImport Matplotlib\n",
        "\n",
        "5.\tDisplaying Plots in Matplotlib\n",
        "\n",
        "6.\tMatplotlib Object Hierarchy\n",
        "\n",
        "7.\tMatplotlib interfaces\n",
        "\n",
        "8.\tPyplot API\n",
        "\n",
        "9.\tObject-Oriented API\n",
        "\n",
        "10.\tFigure and Subplots\n",
        "\n",
        "11.\tFirst plot with Matplotlib\n",
        "\n",
        "12.\tMultiline Plots\n",
        "\n",
        "13.\tParts of a Plot\n",
        "\n",
        "14.\tSaving the Plot\n",
        "\n",
        "15.\tLine Plot\n",
        "\n",
        "16.\tScatter Plot\n",
        "\n",
        "17.\tHistogram\n",
        "\n",
        "18.\tBar Chart\n",
        "\n",
        "19.\tHorizontal Bar Chart\n",
        "\n",
        "20.\tError Bar Chart\n",
        "\n",
        "21.\tMultiple Bar Chart\n",
        "\n",
        "22.\tStacked Bar Chart\n",
        "\n",
        "23.\tBack-to-back Bar Chart\n",
        "\n",
        "24.\tPie Chart\n",
        "\n",
        "25.\tBox Plot\n",
        "\n",
        "26.\tArea Chart\n",
        "\n",
        "27.\tContour Plot\n",
        "\n",
        "28.\tImage Plot\n",
        "\n",
        "29.\tPolar Chart\n",
        "\n",
        "30.\t3D Plotting with Matplotlib\n",
        "\n",
        "31.\tStyles with Matplotlib Plots\n",
        "\n",
        "32.\tAdding a grid\n",
        "\n",
        "33.\tHandling axes\n",
        "\n",
        "34.\tHandling X and Y ticks\n",
        "\n",
        "35.\tAdding labels\n",
        "\n",
        "36.\tAdding a title\n",
        "\n",
        "37.\tAdding a legend\n",
        "\n",
        "38.\tControl colours\n",
        "\n",
        "39.\tControl line styles\n",
        " \n",
        "40.\tSummary\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "1. Introduction\n",
        "\n",
        "\n",
        "When we want to convey some information to others, there are several ways to do so. The process of conveying the information with the help of plots and graphics is called **Data Visualization**. The plots and graphics take numerical data as input and display output in the form of charts, figures and tables. It helps to analyze and visualize the data clearly and make concrete decisions. It makes complex data more accessible and understandable. The goal of data visualization is to communicate information in a clear and efficient manner.\n",
        "\n",
        "\n",
        "In this project, I shed some light on **Matplotlib**, which is the basic data visualization tool of Python programming language. Python has different data visualization tools available which are suitable for different purposes. First of all, I will list these data visualization tools and then I will discuss Matplotlib.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "2. Overview of Python Visualization Tools\n",
        "\n",
        "\n",
        "\n",
        "Python is the preferred language of choice for data scientists. Python have multiple options for data visualization. It has several tools which can help us to visualize the data more effectively. These Python data visualization tools are as follows:-\n",
        "\n",
        "\n",
        "\n",
        "•\tMatplotlib\n",
        "\n",
        "•\tSeaborn\n",
        "\n",
        "•\tpandas\n",
        "\n",
        "•\tBokeh\n",
        "\n",
        "•\tPlotly\n",
        "\n",
        "•\tggplot\n",
        "\n",
        "•\tpygal\n",
        "\n",
        "\n",
        "\n",
        "In the following sections, I discuss Matplotlib as the data visualization tool. \n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "3. Introduction to Matplotlib\n",
        "\n",
        "\n",
        "**Matplotlib** is the basic plotting library of Python programming language. It is the most prominent tool among Python visualization packages. Matplotlib is highly efficient in performing wide range of tasks. It can produce publication quality figures in a variety of formats.  It can export visualizations to all of the common formats like PDF, SVG, JPG, PNG, BMP and GIF. It can create popular visualization types – line plot, scatter plot, histogram, bar chart, error charts, pie chart, box plot, and many more types of plot. Matplotlib also supports 3D plotting. Many Python libraries are built on top of Matplotlib. For example, pandas and Seaborn are built on Matplotlib. They allow to access Matplotlib's methods with less code. \n",
        "\n",
        "\n",
        "The project **Matplotlib** was started by John Hunter in 2002. Matplotlib was originally started to visualize Electrocorticography (ECoG) data of epilepsy patients during post-doctoral research in Neurobiology. The open-source tool Matplotlib emerged as the most widely used plotting library for the Python programming language. It was used for data visualization during landing of the Phoenix spacecraft in 2008.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "\n",
        "4. Import Matplotlib\n",
        "\n",
        "Before, we need to actually start using Matplotlib, we need to import it. We can import Matplotlib as follows:-\n",
        "\n",
        "`import matplotlib`\n",
        "\n",
        "\n",
        "Most of the time, we have to work with **pyplot** interface of Matplotlib. So, I will import **pyplot** interface of Matplotlib as follows:-\n",
        "\n",
        "\n",
        "`import matplotlib.pyplot`\n",
        "\n",
        "\n",
        "To make things even simpler, we will use standard shorthand for Matplotlib imports as follows:-\n",
        "\n",
        "\n",
        "`import matplotlib.pyplot as plt`\n",
        "\n"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 1,
       "metadata": {},
       "outputs": [],
       "source": [
        "Import dependencies\n",
        "\n",
        "import numpy as np\n",
        "import pandas as pd"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 2,
       "metadata": {},
       "outputs": [],
       "source": [
        "Import Matplotlib\n",
        "\n",
        "import matplotlib.pyplot as plt "
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "5. Displaying Plots in Matplotlib\n",
        "\n",
        "\n",
        "Viewing the Matplotlib plot is context based. The best usage of Matplotlib differs depending on how we are using it. \n",
        "There are three applicable contexts for viewing the plots. The three applicable contexts are using plotting from a script, plotting from an IPython shell or plotting from a Jupyter notebook.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "Plotting from a script\n",
        "\n",
        "\n",
        "\n",
        "If we are using Matplotlib from within a script, then the **plt.show()** command is of great use. It starts an event loop, \n",
        "looks for all currently active figure objects, and opens one or more interactive windows that display the figure or figures.\n",
        "\n",
        "\n",
        "The **plt.show()** command should be used only once per Python session. It should be used only at the end of the script. Multiple **plt.show()** commands can lead to unpredictable results and should mostly be avoided.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "Plotting from an IPython shell\n",
        "\n",
        "\n",
        "We can use Matplotlib interactively within an IPython shell. IPython works well with Matplotlib if we specify Matplotlib mode. To enable this mode, we can use the **%matplotlib** magic command after starting ipython. Any plt plot command will cause a figure window to open and further commands can be run to update the plot.\n",
        "\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "Plotting from a Jupyter notebook\n",
        "\n",
        "\n",
        "The Jupyter Notebook (formerly known as the IPython Notebook) is a data analysis and visualization tool that provides multiple tools under one roof.  It provides code execution, graphical plots, rich text and media display, mathematics formula and much more facilities into a single executable document.\n",
        "\n",
        "\n",
        "Interactive plotting within a Jupyter Notebook can be done with the **%matplotlib** command. There are two possible options to work with graphics in Jupyter Notebook. These are as follows:-\n",
        "\n",
        "\n",
        "•\t**%matplotlib notebook** – This command will produce interactive plots embedded within the notebook.\n",
        "\n",
        "•\t**%matplotlib inline** – It will output static images of the plot embedded in the notebook.\n",
        "\n",
        "\n",
        "After this command (it needs to be done only once per kernel per session), any cell within the notebook that creates a plot will embed a PNG image of the graphic.\n"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 3,
       "metadata": {},
       "outputs": [
        {
         "data": {
          "image/png": "i=\n",
          "text/plain": [
           "<Figure size 432x288 with 1 Axes>"
          ]
         },
         "metadata": {
          "needs_background": "light"
         },
         "output_type": "display_data"
        }
       ],
       "source": [
        "%matplotlib inline\n",
        "\n",
        "\n",
        "x1 = np.linspace(0, 10, 100)\n",
        "\n",
        "\n",
        "create a plot figure\n",
        "fig = plt.figure()\n",
        "\n",
        "plt.plot(x1, np.sin(x1), '-')\n",
        "plt.plot(x1, np.cos(x1), '--');"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "6. Matplotlib Object Hierarchy\n",
        "\n",
        "\n",
        "There is an Object Hierarchy within Matplotlib. In Matplotlib, a plot is a hierarchy of nested Python objects. \n",
        "A**hierarch** means that there is a tree-like structure of Matplotlib objects underlying each plot.\n",
        "\n",
        "\n",
        "A **Figure** object is the outermost container for a Matplotlib plot. The **Figure** object contain multiple **Axes** objects. So, the **Figure** is the final graphic that may contain one or more **Axes**. The **Axes** represent an individual plot.\n",
        "\n",
        "\n",
        "So, we can think of the **Figure** object as a box-like container containing one or more **Axes**. The **Axes** object contain smaller objects such as tick marks, lines, legends, title and text-boxes.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "7.\tMatplotlib API Overview\n",
        "\n",
        "\n",
        "\n",
        "Matplotlib has two APIs to work with. A MATLAB-style state-based interface and a more powerful object-oriented (OO) interface. \n",
        "The former MATLAB-style state-based interface is called **pyplot interface** and the latter is called **Object-Oriented** interface.\n",
        "\n",
        "\n",
        "There is a third interface also called **pylab** interface. It merges pyplot (for plotting) and NumPy (for mathematical functions) together in an environment closer to MATLAB. This is considered bad practice nowadays. So, the use of **pylab** is strongly discouraged and hence, I will not discuss it any further.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "8. Pyplot API \n",
        "\n",
        "\n",
        "**Matplotlib.pyplot** provides a MATLAB-style, procedural, state-machine interface to the underlying object-oriented library in Matplotlib. **Pyplot** is a collection of command style functions that make Matplotlib work like MATLAB. Each pyplot function makes some change to a figure - e.g., creates a figure, creates a plotting area in a figure etc. \n",
        "\n",
        "\n",
        "**Matplotlib.pyplot** is stateful because the underlying engine keeps track of the current figure and plotting area information and plotting functions change that information. To make it clearer, we did not use any object references during our plotting we just issued a pyplot command, and the changes appeared in the figure.\n",
        "\n",
        "\n",
        "We can get a reference to the current figure and axes using the following commands-\n",
        "\n",
        "\n",
        "`plt.gcf ( )`   # get current figure\n",
        "\n",
        "`plt.gca ( )`   # get current axes \n",
        "\n",
        " \n",
        "**Matplotlib.pyplot** is a collection of commands and functions that make Matplotlib behave like MATLAB (for plotting). \n",
        "The MATLAB-style tools are contained in the pyplot (plt) interface. \n",
        "\n",
        "This is really helpful for interactive plotting, because we can issue a command and see the result immediately. But, it is not suitable for more complicated cases. For these cases, we have another interface called **Object-Oriented** interface, described later.\n"
       ]
      },
      {
       "cell_type": "markdown",
       "metadata": {},
       "source": [
        "The following code produces sine and cosine curves using Pyplot API."
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 4,
       "metadata": {},
       "outputs": [
        {
         "data": {
          "image/png": "ig==\n",
          "text/plain": [
           "<Figure size 432x288 with 2 Axes>"
          ]
         },
         "metadata": {
          "needs_background": "light"
         },
         "output_type": "display_data"
        }
       ],
       "source": [
        "create a plot figure\n",
        "plt.figure()\n",
        "\n",
        "\n",
        "create the first of two panels and set current axis\n",
        "plt.subplot(2, 1, 1)   # (rows, columns, panel number)\n",
        "plt.plot(x1, np.sin(x1))\n",
        "\n",
        "\n",
        "create the second of two panels and set current axis\n",
        "plt.subplot(2, 1, 2)   # (rows, columns, panel number)\n",
        "plt.plot(x1, np.cos(x1));\n"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 5,
       "metadata": {},
       "outputs": [
        {
         "name": "stdout",
         "output_type": "stream",
         "text": [
          "Figure(432x288)\n"
         ]
        },
        {
         "data": {
          "text/plain": [
           "<Figure size 432x288 with 0 Axes>"
          ]
         },
         "metadata": {},
         "output_type": "display_data"
        }
       ],
       "source": [
        "get current figure information\n",
        "\n",
        "print(plt.gcf())"
       ]
      },
      {
       "cell_type": "code",
       "execution_count": 6,
       "metadata": {},
       "outputs": [
        {
         "name": "stdout",
         "output_type": "stream",
         "text": [
          "AxesSubplot(0.125,0.125;0.775x0.755)\n"
         ]
        },
        {
         "data": {
          "image/png": "=\n",
          "text/plain": [
           "<Figure size 432x288 with 1 Axes>"
          ]
         },
         "metadata": {
          "needs_background": "light"
         },
         "output_type": "display_data"
        }
       ],
       "source": [
        "get current axis information\n",
        "\n",
        "print(plt.gca())"
       ]
      }
     ]
    }"#
}

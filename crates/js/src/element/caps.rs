//! What each element kind takes. `node!(.., style, interactive, parent)` names the same for the
//! widgets of `components/`; keep both in step. A `corona/base` widget is `"Kind@base"`, as it
//! takes other methods than the component of the kind, as the d.ts types the methods of each group on
//! exactly the kinds listed here. `PARENT` is also given to the containers taking typed children
//! (`child` there is the one of `children` in `components/`).

pub(crate) const STYLE: u8 = 1;
pub(crate) const INTERACTIVE: u8 = 2;
pub(crate) const PARENT: u8 = 4;

/// The kinds and what they take
const KINDS: &[(&str, u8)] = &[
  #[cfg(test)]
  ("Probe", STYLE),
  ("Accordion", STYLE | PARENT),
  ("Accordion@base", STYLE | INTERACTIVE | PARENT),
  ("AccordionHeader@base", STYLE | PARENT),
  ("AccordionItem", STYLE | PARENT),
  ("AccordionItem@base", STYLE | PARENT),
  ("AccordionPanel@base", STYLE | PARENT),
  ("AccordionTrigger@base", STYLE | INTERACTIVE | PARENT),
  ("Alert", STYLE),
  ("AlertDialog", STYLE),
  ("AreaChart", STYLE | INTERACTIVE),
  ("Attachment", STYLE | PARENT),
  ("Avatar", STYLE | INTERACTIVE),
  ("Avatar@base", STYLE | INTERACTIVE),
  ("AvatarFallback@base", STYLE | PARENT),
  ("AvatarImage", STYLE | INTERACTIVE),
  ("AvatarImage@base", STYLE | INTERACTIVE),
  ("Badge", STYLE | INTERACTIVE | PARENT),
  ("BarChart", STYLE | INTERACTIVE),
  ("Breadcrumb", STYLE),
  ("Bubble", STYLE | PARENT),
  ("Button", STYLE | INTERACTIVE | PARENT),
  ("Button@base", STYLE | INTERACTIVE | PARENT),
  ("Calendar", STYLE),
  ("Carousel", STYLE | PARENT),
  ("CarouselContent", STYLE | PARENT),
  ("CarouselItem", STYLE | PARENT),
  ("CarouselNext", STYLE | PARENT),
  ("CarouselPagination", STYLE | PARENT),
  ("CarouselPaginationItem", STYLE | PARENT),
  ("CarouselPrevious", STYLE | PARENT),
  ("Checkbox", STYLE | INTERACTIVE | PARENT),
  ("Checkbox@base", STYLE | INTERACTIVE | PARENT),
  ("Clipboard", 0),
  ("Collapsible", STYLE | PARENT),
  ("Collapsible@base", STYLE | PARENT),
  ("ColorPicker", STYLE),
  ("Combobox", STYLE),
  ("Combobox@base", STYLE | PARENT),
  ("Command", STYLE | PARENT),
  ("CommandGroup", PARENT),
  ("CommandItem", 0),
  ("CommandSeparator", 0),
  ("DataTable", STYLE),
  ("DatePicker", STYLE),
  ("DatePicker@base", STYLE | INTERACTIVE | PARENT),
  ("DescriptionItem", 0),
  ("DescriptionList", STYLE | INTERACTIVE | PARENT),
  ("Dialog", STYLE),
  ("Diff", STYLE),
  ("DropdownButton", STYLE),
  ("DropdownMenu", STYLE),
  ("Editor", STYLE),
  ("Empty", STYLE | PARENT),
  ("Empty@base", 0),
  ("EmptyContent", STYLE | PARENT),
  ("EmptyDescription", STYLE | PARENT),
  ("EmptyHeader", STYLE),
  ("EmptyMedia", STYLE | PARENT),
  ("EmptyTitle", STYLE | PARENT),
  ("Field", STYLE | PARENT),
  ("Form", STYLE | PARENT),
  ("GroupBox", STYLE | PARENT),
  ("HoverCard", STYLE | PARENT),
  ("HoverCard@base", 0),
  ("Icon", STYLE),
  ("Input", STYLE),
  ("Input@base", 0),
  ("InputGroup", STYLE),
  ("InputGroupAddon", STYLE | PARENT),
  ("InputGroupButton", STYLE | INTERACTIVE | PARENT),
  ("InputGroupText", STYLE | PARENT),
  ("Kbd", STYLE),
  ("Label", STYLE | INTERACTIVE),
  ("LineChart", STYLE | INTERACTIVE),
  ("Link", STYLE | PARENT),
  ("Link@base", STYLE | INTERACTIVE | PARENT),
  ("List", STYLE),
  ("Marker", STYLE | PARENT),
  ("Menu", PARENT),
  ("MenuBar", PARENT),
  ("MenuItem", 0),
  ("MenuSeparator", 0),
  ("Message", STYLE | PARENT),
  ("MessageScroller", STYLE),
  ("NativeMenuItem", 0),
  ("NativeMenuSeparator", 0),
  ("NativeMenuTrigger", STYLE | PARENT),
  ("Notification", STYLE),
  ("NumberInput", STYLE),
  ("NumberInput@base", STYLE | PARENT),
  ("OtpInput", 0),
  ("OtpInput@base", STYLE | PARENT),
  ("Pagination", STYLE),
  ("Pagination@base", STYLE | INTERACTIVE | PARENT),
  ("PieChart", STYLE | INTERACTIVE),
  ("Popover", STYLE | PARENT),
  ("Popover@base", STYLE),
  ("Popup@base", STYLE | INTERACTIVE),
  ("Probe", STYLE),
  ("Progress", STYLE),
  ("Progress@base", STYLE | INTERACTIVE | PARENT),
  ("ProgressIndicator@base", STYLE | PARENT),
  ("ProgressTrack@base", STYLE | PARENT),
  ("Questionnaire", STYLE | PARENT),
  ("QuestionnaireChoice", 0),
  ("QuestionnaireInput", 0),
  ("QuestionnaireItem", PARENT),
  ("RadarChart", STYLE | INTERACTIVE),
  ("Radio", STYLE | INTERACTIVE | PARENT),
  ("Radio@base", STYLE | INTERACTIVE | PARENT),
  ("RadioGroup", STYLE | PARENT),
  ("RadioGroup@base", STYLE | INTERACTIVE | PARENT),
  ("Rating", STYLE),
  ("Resizable", STYLE | PARENT),
  ("Resizable@base", STYLE | PARENT),
  ("ResizablePanel", STYLE | PARENT),
  ("ResizablePanel@base", STYLE | PARENT),
  ("Scrollable", STYLE | INTERACTIVE | PARENT),
  ("Scroll", STYLE | INTERACTIVE | PARENT),
  ("Scrollbar", 0),
  ("Scrollbar@base", 0),
  ("Select", STYLE),
  ("Select@base", STYLE | PARENT),
  ("Separator", STYLE),
  ("SettingGroup", STYLE | PARENT),
  ("SettingItem", 0),
  ("SettingPage", PARENT),
  ("Settings", PARENT),
  ("Sheet", STYLE),
  ("ShimmerText", STYLE),
  ("Sidebar", STYLE | PARENT),
  ("SidebarFooter", STYLE | INTERACTIVE | PARENT),
  ("SidebarHeader", STYLE | INTERACTIVE | PARENT),
  ("SidebarMenu", PARENT),
  ("SidebarMenuItem", PARENT),
  ("SidebarToggleButton", 0),
  ("Skeleton", STYLE),
  ("Slider", STYLE),
  ("Slider@base", STYLE | PARENT),
  ("SliderIndicator@base", STYLE | PARENT),
  ("SliderThumb@base", STYLE | PARENT),
  ("SliderTrack@base", STYLE | INTERACTIVE | PARENT),
  ("Spinner", STYLE | INTERACTIVE),
  ("StatusBar", STYLE | PARENT),
  ("Stepper", STYLE | PARENT),
  ("StepperItem", STYLE | PARENT),
  ("Switch", STYLE),
  ("Switch@base", STYLE | INTERACTIVE | PARENT),
  ("Tab", STYLE | INTERACTIVE | PARENT),
  ("Tab@base", STYLE | INTERACTIVE | PARENT),
  ("TabBar", STYLE | PARENT),
  ("Table", STYLE | PARENT),
  ("Table@base", STYLE | INTERACTIVE | PARENT),
  ("TableBody", STYLE | PARENT),
  ("TableBody@base", STYLE | INTERACTIVE | PARENT),
  ("TableCaption", STYLE | PARENT),
  ("TableCaption@base", STYLE | INTERACTIVE | PARENT),
  ("TableCell", STYLE | PARENT),
  ("TableCell@base", STYLE | INTERACTIVE | PARENT),
  ("TableFooter", STYLE | PARENT),
  ("TableHead", STYLE | PARENT),
  ("TableHead@base", STYLE | INTERACTIVE | PARENT),
  ("TableHeader", STYLE | PARENT),
  ("TableHeader@base", STYLE | INTERACTIVE | PARENT),
  ("TableRow", STYLE | PARENT),
  ("TableRow@base", STYLE | INTERACTIVE | PARENT),
  ("Tabs@base", STYLE | INTERACTIVE | PARENT),
  ("Tag", STYLE | PARENT),
  ("TextView@base", STYLE),
  ("Textarea", STYLE),
  ("Textarea@base", 0),
  ("TimeField", STYLE),
  ("Toggle", STYLE | PARENT),
  ("Toggle@base", STYLE | INTERACTIVE | PARENT),
  ("ToggleGroup@base", STYLE | INTERACTIVE | PARENT),
  ("Toolbar", STYLE | PARENT),
  ("Tree", STYLE | PARENT),
  ("TreeItem", PARENT),
  ("VirtualList@base", STYLE),
  ("canvas", STYLE),
  ("div", STYLE | INTERACTIVE | PARENT),
  ("dock_area@base", STYLE),
  ("dock_content", STYLE),
  ("dock_content@base", STYLE),
  ("fps_monitor", 0),
  ("image", STYLE | INTERACTIVE),
  ("list", STYLE),
  ("style", STYLE),
  ("svg", STYLE | INTERACTIVE),
  ("uniform_list", STYLE | INTERACTIVE),
];

/// What the kind takes, `STYLE | INTERACTIVE | PARENT`; none for an unknown kind
#[cfg(test)]
pub(crate) fn caps(kind: &str) -> Option<u8> {
  KINDS.iter().find(|(k, _)| *k == kind).map(|(_, c)| *c)
}

/// `"A" | "B"`: the kinds taking `group` (`STYLE`, `INTERACTIVE` or `PARENT`)
fn union(group: u8) -> String {
  let names: Vec<String> = KINDS
    .iter()
    .filter(|(_, caps)| caps & group != 0)
    .map(|(kind, _)| format!("\"{kind}\""))
    .collect();
  names.join(" | ")
}

/// `type StyledKind = ..`, `InteractiveKind`, `ParentKind`, and `ElementOf<..>`, an element by
/// what it takes instead of by kind
pub(crate) fn d_ts() -> String {
  format!(
    "  /** The kinds of element taking style methods. */
  export type StyledKind = {};
  /** The kinds of element taking `id`, `on_click`, .. */
  export type InteractiveKind = {};
  /** The kinds of element taking `child`. */
  export type ParentKind = {};
  /** What an element takes: style methods, interaction methods, `child`. */
  export type Capability = \"style\" | \"interactive\" | \"parent\";
  /**
   * An element of any kind taking all of `C`, e.g. `ElementOf<\"style\" | \"parent\">` for a helper
   * returning a `div()`, an `h_flex()` or a `Button`.
   */
  export type ElementOf<C extends Capability> = Element<
    (\"style\" extends C ? StyledKind : string) &
      (\"interactive\" extends C ? InteractiveKind : string) &
      (\"parent\" extends C ? ParentKind : string)
  >;
",
    union(STYLE),
    union(INTERACTIVE),
    union(PARENT),
  )
}

//! Reusable UI components.

mod header;
mod sidebar;
mod toast;
mod modal;
mod table;
mod searchable_table;
mod card;
mod button;
mod input;
mod toggle;
mod gauge;
mod icon;
mod loading;
mod policy_editor;

pub use header::Header;
pub use icon::{get_icon_by_name, Icon, IconName, IconSize};
pub use loading::{CardSkeleton, LoadingFallback, LoadingSize, LoadingSpinner, PageLoading, TableRowSkeleton};
pub use sidebar::{Sidebar, SidebarState};
pub use toast::ToastContainer;
pub use modal::{Modal, ConfirmModal, ConfirmState};
pub use table::{SrOnly, Table, TableRow};
pub use searchable_table::{
    CardEmptyState, CardField, CardStatus, ResponsiveCard, ResponsiveDataView,
    SearchableTableHeader, SortDirection, SortState, StatusBadge, TableColumn,
    TableConfig, TableEmptyState, TableResultsCount, TableSearchInput,
};
pub use card::Card;
pub use button::{Button, ButtonSize, ButtonVariant, IconButton};
pub use input::{Input, InputSize};
pub use toggle::{Toggle, ToggleCompact};
pub use gauge::{CircularGauge, CircularGaugeMini, GaugeSize};
pub use policy_editor::{PolicyEditor, PolicyValidation, PolicyViewer};

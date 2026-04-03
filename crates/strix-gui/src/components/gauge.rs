//! Circular gauge component for visualizing percentages.

use leptos::prelude::*;

/// Size presets for the circular gauge.
#[derive(Clone, Copy, Default)]
pub enum GaugeSize {
    /// Small gauge (64x64).
    Small,
    /// Medium gauge (96x96) - default.
    #[default]
    Medium,
    /// Large gauge (128x128).
    Large,
}

impl GaugeSize {
    fn dimensions(&self) -> (u32, u32, u32, &'static str) {
        // (radius, center, stroke_width, svg_class)
        match self {
            GaugeSize::Small => (24, 32, 6, "w-16 h-16"),
            GaugeSize::Medium => (40, 48, 8, "w-24 h-24"),
            GaugeSize::Large => (52, 64, 10, "w-32 h-32"),
        }
    }

    fn text_class(&self) -> &'static str {
        match self {
            GaugeSize::Small => "text-sm",
            GaugeSize::Medium => "text-lg",
            GaugeSize::Large => "text-xl",
        }
    }

    fn label_class(&self) -> &'static str {
        match self {
            GaugeSize::Small => "text-xs",
            GaugeSize::Medium => "text-sm",
            GaugeSize::Large => "text-base",
        }
    }
}

/// A circular gauge for visualizing percentage values.
#[component]
pub fn CircularGauge(
    /// Percentage value (0-100).
    percentage: u32,
    /// Tailwind color class for the gauge (e.g., "text-strix-400").
    color: &'static str,
    /// Label text displayed below the gauge.
    label: String,
    /// Value text displayed in the center of the gauge.
    value: String,
    /// Size of the gauge.
    #[prop(optional)]
    size: GaugeSize,
    /// Whether to animate the gauge on load.
    #[prop(optional)]
    animated: bool,
) -> impl IntoView {
    let (radius, center, stroke_width, svg_class) = size.dimensions();
    let text_class = size.text_class();
    let label_class = size.label_class();

    // SVG circle parameters
    let circumference = 2.0 * std::f64::consts::PI * (radius as f64);
    let clamped_pct = percentage.min(100);
    let stroke_dashoffset = circumference - (circumference * clamped_pct as f64 / 100.0);

    let animation_class = if animated {
        "transition-all duration-1000 ease-out"
    } else {
        ""
    };

    view! {
        <div class="flex flex-col items-center">
            <div class="relative">
                <svg class=format!("{} transform -rotate-90", svg_class)>
                    // Background circle
                    <circle
                        class="text-slate-700"
                        stroke-width=stroke_width
                        stroke="currentColor"
                        fill="transparent"
                        r=radius
                        cx=center
                        cy=center
                    />
                    // Progress circle
                    <circle
                        class=format!("{} {}", color, animation_class)
                        stroke-width=stroke_width
                        stroke-dasharray=circumference
                        stroke-dashoffset=stroke_dashoffset
                        stroke-linecap="round"
                        stroke="currentColor"
                        fill="transparent"
                        r=radius
                        cx=center
                        cy=center
                    />
                </svg>
                // Center value
                <div class="absolute inset-0 flex items-center justify-center">
                    <span class=format!("{} font-bold text-white", text_class)>{value}</span>
                </div>
            </div>
            <span class=format!("mt-2 {} text-slate-400", label_class)>{label}</span>
        </div>
    }
}

/// A mini circular gauge for inline use (no label).
#[component]
pub fn CircularGaugeMini(
    /// Percentage value (0-100).
    percentage: u32,
    /// Tailwind color class for the gauge.
    color: &'static str,
    /// Optional value to display in center (if omitted, shows percentage).
    #[prop(optional)]
    value: Option<String>,
) -> impl IntoView {
    let radius = 16;
    let center = 20;
    let stroke_width = 4;
    let circumference = 2.0 * std::f64::consts::PI * (radius as f64);
    let clamped_pct = percentage.min(100);
    let stroke_dashoffset = circumference - (circumference * clamped_pct as f64 / 100.0);

    let display_value = value.unwrap_or_else(|| format!("{}%", clamped_pct));

    view! {
        <div class="relative inline-flex items-center justify-center w-10 h-10">
            <svg class="w-10 h-10 transform -rotate-90">
                <circle
                    class="text-slate-700"
                    stroke-width=stroke_width
                    stroke="currentColor"
                    fill="transparent"
                    r=radius
                    cx=center
                    cy=center
                />
                <circle
                    class=color
                    stroke-width=stroke_width
                    stroke-dasharray=circumference
                    stroke-dashoffset=stroke_dashoffset
                    stroke-linecap="round"
                    stroke="currentColor"
                    fill="transparent"
                    r=radius
                    cx=center
                    cy=center
                />
            </svg>
            <span class="absolute text-xs font-medium text-white">{display_value}</span>
        </div>
    }
}

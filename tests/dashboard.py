#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.12"
# dependencies = ["streamlit", "plotly", "pandas", "duckdb"]
# ///

try:
    from streamlit.runtime.scriptrunner import get_script_run_ctx

    if get_script_run_ctx() is None:
        raise RuntimeError("Not in streamlit context")
except Exception:
    import subprocess
    import sys

    cmd = ["uv", "run", "streamlit", "run", sys.argv[0]]
    try:
        sys.exit(subprocess.call(cmd))
    except KeyboardInterrupt:
        sys.exit(130)

import duckdb
import plotly.express as px
import plotly.graph_objects as go
import streamlit as st

st.set_page_config(page_title="MotherDuck Scale Test", layout="wide")
st.title("MotherDuck Scale Test Dashboard")

LOGS_GLOB = "./logs/**/*.json"

# Load all runs (ignore_errors to skip malformed JSON)
try:
    all_data = duckdb.sql(
        f"SELECT * FROM read_json_auto('{LOGS_GLOB}', ignore_errors=true, union_by_name=true)"
    ).fetchdf()
except Exception as e:
    st.error(f"No data found. Run `./test-scale.sh` first.\n\n{e}")
    st.stop()

if all_data.empty:
    st.warning("No log files found in ./logs/")
    st.stop()

# Ensure new columns exist for backward compat with old runs
for col in ["select2_ms", "select3_ms"]:
    if col not in all_data.columns:
        all_data[col] = None

STEP_COLS = [
    "create_sa_ms",
    "set_duckling_ms",
    "create_token_ms",
    "select1_ms",
    "select2_ms",
    "select3_ms",
    "pragma_ms",
    "cleanup_ms",
]
STEP_LABELS = {
    "create_sa_ms": "Create SA",
    "set_duckling_ms": "Set Duckling",
    "create_token_ms": "Create Token",
    "select1_ms": "SELECT 1 (cold)",
    "select2_ms": "SELECT 2 (warm)",
    "select3_ms": "SELECT 3 (hot)",
    "pragma_ms": "PRAGMA",
    "cleanup_ms": "Cleanup",
}

# Sidebar: run picker
runs = sorted(all_data["run_id"].unique(), reverse=True)
run_labels = {
    r: f"{r} ({len(all_data[all_data['run_id'] == r])} workers)" for r in runs
}

st.sidebar.header("Run selection")
selected_run = st.sidebar.selectbox(
    "Run",
    runs,
    index=0,
    format_func=lambda r: run_labels[r],
)
compare_run = st.sidebar.selectbox(
    "Compare with",
    [None] + [r for r in runs if r != selected_run],
    index=0,
    format_func=lambda r: run_labels[r] if r else "None",
)

df = all_data[all_data["run_id"] == selected_run].copy().sort_values("worker")
n_workers = len(df)
n_passed = len(df[df["status"] == "success"])
n_failed = len(df[df["status"] == "failed"])

# Top-line metrics
st.subheader("Summary")
cols = st.columns(6)
cols[0].metric("Workers", n_workers)
cols[1].metric("Passed", n_passed)
cols[2].metric(
    "Failed",
    n_failed,
    delta=f"-{n_failed}" if n_failed else None,
    delta_color="inverse",
)
cols[3].metric("Avg Total", f"{df['total_ms'].mean():.0f}ms")
cols[4].metric("p50 Total", f"{df['total_ms'].quantile(0.5):.0f}ms")
cols[5].metric("p95 Total", f"{df['total_ms'].quantile(0.95):.0f}ms")

# Percentile table
st.subheader("Latency percentiles (ms)")
percentiles = [0.25, 0.50, 0.75, 0.90, 0.95, 0.99, 1.0]
perc_data = {}
for col in ["total_ms"] + STEP_COLS:
    valid = df[col].dropna()
    if valid.empty:
        continue
    label = STEP_LABELS.get(col, "Total")
    perc_data[label] = {
        "min": valid.min(),
        **{f"p{int(p * 100)}": valid.quantile(p) for p in percentiles},
        "mean": valid.mean(),
    }
perc_df = (
    __import__("pandas")
    .DataFrame(perc_data)
    .T.rename(columns={"p100": "max"})
    .round(0)
    .astype(int)
)
st.dataframe(perc_df, width="stretch")

# Worker outcome by index
st.subheader("Pass / fail by worker index")
status_color = (
    df["status"].map({"success": "#2ecc71", "failed": "#e74c3c"}).fillna("#95a5a6")
)
fig_outcome = go.Figure()
fig_outcome.add_trace(
    go.Bar(
        x=df["worker"],
        y=[1] * len(df),
        marker_color=status_color,
        hovertext=df.apply(
            lambda r: (
                f"Worker {int(r['worker'])}: {r['status']}"
                + (f"\n{r['error']}" if r.get("error") else "")
            ),
            axis=1,
        ),
        hoverinfo="text",
    )
)
# Determine which step failed from the error message
ERROR_PREFIX_TO_STEP = {
    "create service account": "Create SA",
    "set duckling config": "Set Duckling",
    "create token": "Create Token",
    "token parse": "Create Token",
    "SELECT 1": "SELECT 1 (cold)",
    "SELECT 2": "SELECT 2 (warm)",
    "SELECT 3": "SELECT 3 (hot)",
    "pragma print_md_token": "PRAGMA",
    "token mismatch": "PRAGMA",
}


def detect_failed_step(error: str) -> str:
    if not error:
        return ""
    for prefix, step in ERROR_PREFIX_TO_STEP.items():
        if prefix in error:
            return step
    return error[:40]


df["failed_step"] = df.apply(
    lambda r: detect_failed_step(r["error"]) if r["status"] == "failed" else "",
    axis=1,
)
fig_outcome.update_layout(
    yaxis=dict(visible=False, range=[0, 1.2]),
    xaxis_title="Worker #",
    bargap=0,
    height=150,
    margin=dict(t=10, b=40, l=40, r=20),
    showlegend=False,
)
st.plotly_chart(fig_outcome, width="stretch")

if n_failed > 0:
    # Show which step each failure occurred at
    fail_step_counts = (
        df[df["status"] == "failed"]["failed_step"].value_counts().reset_index()
    )
    fail_step_counts.columns = ["Failed at step", "Count"]
    fc1, fc2 = st.columns([1, 2])
    with fc1:
        st.dataframe(fail_step_counts, hide_index=True, width="stretch")
    with fc2:
        # Cumulative failure rate by worker index
        df_sorted = df.sort_values("worker")
        df_sorted["cum_failures"] = (df_sorted["status"] == "failed").cumsum()
        df_sorted["cum_fail_pct"] = (
            df_sorted["cum_failures"] / (df_sorted.index.values + 1) * 100
        )
        # Recompute using position
        positions = range(1, len(df_sorted) + 1)
        cum_fail_pct = [
            df_sorted.iloc[:i]["status"].eq("failed").sum() / i * 100 for i in positions
        ]
        fig_cum = go.Figure(
            go.Scatter(
                x=df_sorted["worker"],
                y=cum_fail_pct,
                mode="lines",
                fill="tozeroy",
                line=dict(color="#e74c3c"),
            )
        )
        fig_cum.update_layout(
            xaxis_title="Worker #",
            yaxis_title="Cumulative failure %",
            height=250,
            margin=dict(t=10, b=40),
        )
        st.plotly_chart(fig_cum, width="stretch")

# Charts row 1: total latency distribution + per-step avg breakdown
c1, c2 = st.columns(2)

with c1:
    st.subheader("Total latency distribution")
    fig_hist = px.histogram(
        df,
        x="total_ms",
        nbins=30,
        color="status",
        color_discrete_map={"success": "#2ecc71", "failed": "#e74c3c"},
        labels={"total_ms": "Total (ms)", "count": "Workers"},
    )
    fig_hist.update_layout(bargap=0.05, showlegend=True)
    st.plotly_chart(fig_hist, width="stretch")

with c2:
    st.subheader("Average time per step")
    step_avgs = {}
    step_colors_used = []
    step_color_map = dict(
        zip(
            STEP_COLS,
            [
                "#3498db",
                "#9b59b6",
                "#e67e22",
                "#e74c3c",
                "#c0392b",
                "#d35400",
                "#1abc9c",
                "#95a5a6",
            ],
        )
    )
    for c in STEP_COLS:
        v = df[c].dropna()
        if not v.empty:
            step_avgs[STEP_LABELS[c]] = v.mean()
            step_colors_used.append(step_color_map[c])
    fig_bar = go.Figure(
        go.Bar(
            x=list(step_avgs.values()),
            y=list(step_avgs.keys()),
            orientation="h",
            marker_color=step_colors_used,
            text=[f"{v:.0f}ms" for v in step_avgs.values()],
            textposition="auto",
        )
    )
    fig_bar.update_layout(xaxis_title="ms", yaxis=dict(autorange="reversed"))
    st.plotly_chart(fig_bar, width="stretch")

# Charts row 2: per-step box plots + worker scatter
c3, c4 = st.columns(2)

with c3:
    st.subheader("Step latency distributions")
    melted = (
        df[STEP_COLS].rename(columns=STEP_LABELS).melt(var_name="Step", value_name="ms")
    )
    melted = melted.dropna()
    step_order = list(STEP_LABELS.values())
    fig_box = px.box(
        melted,
        x="Step",
        y="ms",
        color="Step",
        category_orders={"Step": step_order},
    )
    fig_box.update_layout(showlegend=False, yaxis_title="ms")
    st.plotly_chart(fig_box, width="stretch")

with c4:
    st.subheader("Latency by worker index")
    fig_scatter = px.scatter(
        df,
        x="worker",
        y="total_ms",
        color="status",
        color_discrete_map={"success": "#2ecc71", "failed": "#e74c3c"},
        labels={"worker": "Worker #", "total_ms": "Total (ms)"},
        hover_data=STEP_COLS,
    )
    fig_scatter.update_layout(showlegend=True)
    st.plotly_chart(fig_scatter, width="stretch")

# Stacked bar: per-worker step breakdown
st.subheader("Per-worker step breakdown")
fig_stacked = go.Figure()
colors = [
    "#3498db",
    "#9b59b6",
    "#e67e22",
    "#e74c3c",
    "#c0392b",
    "#d35400",
    "#1abc9c",
    "#95a5a6",
]
for col, color in zip(STEP_COLS, colors):
    fig_stacked.add_trace(
        go.Bar(
            name=STEP_LABELS[col],
            x=df["worker"],
            y=df[col],
            marker_color=color,
        )
    )
fig_stacked.update_layout(
    barmode="stack",
    xaxis_title="Worker #",
    yaxis_title="ms",
    legend=dict(orientation="h", y=1.12),
)
st.plotly_chart(fig_stacked, width="stretch")

# Failures detail
if n_failed > 0:
    st.subheader("Failures")
    failed_df = df[df["status"] == "failed"][
        ["worker", "service_account", "error", "total_ms"]
    ]
    st.dataframe(failed_df, width="stretch", hide_index=True)

    fail_reasons = failed_df["error"].value_counts().reset_index()
    fail_reasons.columns = ["Error", "Count"]
    fig_fail = px.bar(fail_reasons, x="Count", y="Error", orientation="h")
    st.plotly_chart(fig_fail, width="stretch")

# Cross-run comparison
if compare_run:
    st.divider()
    st.subheader(f"Comparison: {selected_run} vs {compare_run}")
    df2 = all_data[all_data["run_id"] == compare_run].copy()

    comp_items = [("Total", "total_ms")] + [(STEP_LABELS[c], c) for c in STEP_COLS]
    comp_cols = st.columns(len(comp_items))
    for i, (label, col) in enumerate(comp_items):
        curr = df[col].dropna()
        prev = df2[col].dropna()
        if curr.empty:
            comp_cols[i].metric(f"{label} (p50)", "n/a")
            continue
        curr_med = curr.median()
        delta_str = None
        if not prev.empty:
            delta_str = f"{curr_med - prev.median():+.0f}ms"
        comp_cols[i].metric(
            f"{label} (p50)",
            f"{curr_med:.0f}ms",
            delta=delta_str,
            delta_color="inverse",
        )

    comp_data = []
    for col in ["total_ms"] + STEP_COLS:
        label = STEP_LABELS.get(col, "Total")
        comp_data.append(
            {
                "Step": label,
                "Run": str(selected_run),
                "p50": df[col].quantile(0.5),
                "p95": df[col].quantile(0.95),
            }
        )
        comp_data.append(
            {
                "Step": label,
                "Run": str(compare_run),
                "p50": df2[col].quantile(0.5),
                "p95": df2[col].quantile(0.95),
            }
        )

    comp_df = __import__("pandas").DataFrame(comp_data)
    fig_comp = px.bar(
        comp_df,
        x="Step",
        y="p50",
        color="Run",
        barmode="group",
        text="p50",
        labels={"p50": "p50 (ms)"},
    )
    fig_comp.update_traces(texttemplate="%{text:.0f}", textposition="outside")
    fig_comp.update_layout(yaxis_title="p50 latency (ms)")
    st.plotly_chart(fig_comp, width="stretch")

# All runs trend
if len(runs) > 1:
    st.divider()
    st.subheader("Trends across runs")
    trend_data = []
    for r in runs:
        rdf = all_data[all_data["run_id"] == r]
        trend_data.append(
            {
                "run_id": r,
                "workers": len(rdf),
                "pass_rate": len(rdf[rdf["status"] == "success"]) / len(rdf) * 100,
                "p50_total": rdf["total_ms"].quantile(0.5),
                "p95_total": rdf["total_ms"].quantile(0.95),
                "p50_select1": rdf["select1_ms"].dropna().quantile(0.5)
                if not rdf["select1_ms"].dropna().empty
                else None,
                "p95_select1": rdf["select1_ms"].dropna().quantile(0.95)
                if not rdf["select1_ms"].dropna().empty
                else None,
                "p50_select3": rdf["select3_ms"].dropna().quantile(0.5)
                if not rdf["select3_ms"].dropna().empty
                else None,
                "p95_select3": rdf["select3_ms"].dropna().quantile(0.95)
                if not rdf["select3_ms"].dropna().empty
                else None,
            }
        )
    trend_df = __import__("pandas").DataFrame(trend_data).sort_values("run_id")
    trend_df["run_label"] = trend_df["run_id"].astype(str)

    t1, t2 = st.columns(2)
    with t1:
        fig_trend = go.Figure()
        fig_trend.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["p50_total"],
                name="p50 total",
                mode="lines+markers",
            )
        )
        fig_trend.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["p95_total"],
                name="p95 total",
                mode="lines+markers",
            )
        )
        fig_trend.update_layout(
            yaxis_title="ms", xaxis_title="Run ID", title="Total latency trend"
        )
        st.plotly_chart(fig_trend, width="stretch")
    with t2:
        fig_trend2 = go.Figure()
        fig_trend2.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["p50_select1"],
                name="p50 cold",
                mode="lines+markers",
            )
        )
        fig_trend2.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["p50_select3"],
                name="p50 hot",
                mode="lines+markers",
            )
        )
        fig_trend2.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["p95_select1"],
                name="p95 cold",
                mode="lines+markers",
                line=dict(dash="dot"),
            )
        )
        fig_trend2.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["p95_select3"],
                name="p95 hot",
                mode="lines+markers",
                line=dict(dash="dot"),
            )
        )
        fig_trend2.add_trace(
            go.Scatter(
                x=trend_df["run_label"],
                y=trend_df["pass_rate"],
                name="Pass %",
                mode="lines+markers",
                yaxis="y2",
            )
        )
        fig_trend2.update_layout(
            yaxis_title="ms",
            yaxis2=dict(title="Pass %", overlaying="y", side="right", range=[0, 105]),
            xaxis_title="Run ID",
            title="SELECT cold vs hot latency + pass rate trend",
        )
        st.plotly_chart(fig_trend2, width="stretch")

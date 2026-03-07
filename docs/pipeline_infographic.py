#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Chardet-Rust Pipeline Infographic Generator
Creates a visual flowchart of the multi-stage encoding detection pipeline.
"""

import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
from matplotlib.patches import FancyBboxPatch, FancyArrowPatch
import numpy as np

# Set up the figure with a dark professional theme
plt.style.use('dark_background')
fig, ax = plt.subplots(figsize=(20, 28))
ax.set_xlim(0, 20)
ax.set_ylim(0, 28)
ax.axis('off')
fig.patch.set_facecolor('#1a1a2e')
ax.set_facecolor('#1a1a2e')

# Color scheme
colors = {
    'stage0': '#e94560',
    'stage1': '#0f3460',
    'stage2': '#533483',
    'stage3': '#16213e',
    'header': '#e94560',
    'text': '#eaeaea',
    'subtext': '#a0a0a0',
    'arrow': '#4a4a6a',
    'success': '#00d9ff',
    'warning': '#ffc107',
    'binary': '#6c757d',
}

def draw_box(ax, x, y, width, height, color, title, subtitle=None, alpha=0.9):
    box = FancyBboxPatch((x, y), width, height,
                         boxstyle="round,pad=0.02,rounding_size=0.2",
                         facecolor=color, edgecolor='white', linewidth=2,
                         alpha=alpha, zorder=2)
    ax.add_patch(box)
    ax.text(x + width/2, y + height - 0.3, title,
            ha='center', va='top', fontsize=10, fontweight='bold',
            color='white', zorder=3)
    if subtitle:
        ax.text(x + width/2, y + height/2, subtitle,
                ha='center', va='center', fontsize=8,
                color=colors['subtext'], zorder=3)
    return box

def draw_stage_header(ax, y, stage_num, title, color):
    circle = plt.Circle((1.5, y), 0.6, facecolor=color, edgecolor='white', linewidth=2, zorder=3)
    ax.add_patch(circle)
    ax.text(1.5, y, str(stage_num), ha='center', va='center',
            fontsize=14, fontweight='bold', color='white', zorder=4)
    ax.text(2.5, y, title, ha='left', va='center',
            fontsize=16, fontweight='bold', color=color, zorder=3)
    ax.plot([2.5, 18.5], [y - 0.3, y - 0.3], color=color, linewidth=2, alpha=0.5, zorder=1)

# ==================== TITLE ====================
ax.text(10, 27, 'Chardet-Rust', ha='center', va='top',
        fontsize=36, fontweight='bold', color=colors['header'])
ax.text(10, 26.2, 'Multi-Stage Character Encoding Detection Pipeline',
        ha='center', va='top', fontsize=16, color=colors['text'])
ax.text(10, 25.7, 'Universal Charset Detection with Progressive Analysis',
        ha='center', va='top', fontsize=11, color=colors['subtext'], style='italic')

# Input box
draw_box(ax, 8, 24.5, 4, 0.8, '#2d3436', 'INPUT: Raw Bytes', alpha=0.7)

# ==================== STAGE 0 ====================
draw_stage_header(ax, 23.5, 0, 'EARLY DETERMINISTIC DETECTION', colors['stage0'])

box_bom = draw_box(ax, 2, 21.5, 3.5, 1.5, colors['stage0'], 
                   'BOM Detection', 'UTF-8/16/32\nByte Order Mark')
ax.text(3.75, 21.7, 'Confidence: 1.0', ha='center', fontsize=7, 
        color=colors['success'], fontweight='bold')

box_utf16 = draw_box(ax, 6, 21.5, 3.5, 1.5, colors['stage0'],
                     'UTF-16/32 Patterns', 'Pattern-based\nNo BOM required')
ax.text(7.75, 21.7, 'Confidence: 0.95', ha='center', fontsize=7,
        color=colors['success'], fontweight='bold')

box_binary = draw_box(ax, 10, 21.5, 3.5, 1.5, colors['binary'],
                      'Binary Detection', 'Magic numbers\nControl chars')
ax.text(11.75, 21.7, 'Returns: None', ha='center', fontsize=7,
        color=colors['warning'], fontweight='bold')

box_escape = draw_box(ax, 14, 21.5, 4, 1.5, colors['stage0'],
                      'Escape Sequences', 'ISO-2022, HZ-GB-2312, UTF-7')
ax.text(16, 21.7, 'Confidence: 0.95', ha='center', fontsize=7,
        color=colors['success'], fontweight='bold')

ax.text(19, 22.5, '⚡', ha='center', va='center', fontsize=20)
ax.text(19, 21.8, 'Early\nExit', ha='center', va='center', fontsize=7, color=colors['success'])

# ==================== STAGE 1 ====================
draw_stage_header(ax, 20.5, 1, 'MARKUP & BASIC TEXT', colors['stage1'])

box_markup = draw_box(ax, 2, 18.5, 3.5, 1.5, colors['stage1'],
                      'Markup Analysis', 'HTML/XML charset\nMeta tag extraction')
ax.text(3.75, 18.7, 'Confidence: 0.95', ha='center', fontsize=7,
        color=colors['success'], fontweight='bold')

box_ascii = draw_box(ax, 6, 18.5, 3.5, 1.5, colors['stage1'],
                     'ASCII Detection', 'Pure 7-bit text\nFast path')
ax.text(7.75, 18.7, 'Confidence: 1.0', ha='center', fontsize=7,
        color=colors['success'], fontweight='bold')

box_utf8 = draw_box(ax, 10, 18.5, 3.5, 1.5, colors['stage1'],
                    'UTF-8 Validation', 'RFC 3629 compliant\nSequence validation')
ax.text(11.75, 18.7, 'Conf: 0.80-0.99', ha='center', fontsize=7,
        color=colors['success'], fontweight='bold')

# UTF-8 details
utf8_details = ['C2-DF: 2-byte seq', 'E0-EF: 3-byte seq', 'F0-F4: 4-byte seq']
for i, detail in enumerate(utf8_details):
    ax.text(14, 19.3 - i*0.25, f'* {detail}', fontsize=6, 
            color=colors['subtext'], va='top')

# ==================== STAGE 2 ====================
draw_stage_header(ax, 17.5, 2, 'STRUCTURAL ANALYSIS', colors['stage2'])

box_validity = draw_box(ax, 2, 15.5, 3.5, 1.5, colors['stage2'],
                        'Byte Validity', 'Encoding-specific\nFilter candidates')

box_struct = draw_box(ax, 6, 15.5, 4, 1.5, colors['stage2'],
                      'CJK Structural Analysis', 'Multi-byte probing\nLead byte diversity')

struct_metrics = ['Valid sequence ratio', 'Byte coverage >=35%', 'Lead diversity >=4']
for i, metric in enumerate(struct_metrics):
    ax.text(11, 16.6 - i*0.3, f'> {metric}', fontsize=7,
            color=colors['subtext'], va='top')

ax.text(14, 16.5, 'GATING', ha='center', va='center', 
        fontsize=10, color=colors['warning'], fontweight='bold')
ax.text(14, 16.0, 'Skip if <5% MB', ha='center', va='center',
        fontsize=7, color=colors['subtext'])

encodings_box = 'SUPPORTED ENCODINGS:\nShift_JIS * EUC-JP * EUC-KR\nGB18030 * Big5 * Johab\nHZ-GB-2312'
ax.text(17.5, 16.5, encodings_box, ha='center', va='center',
        fontsize=6, color=colors['text'], family='monospace',
        bbox=dict(boxstyle='round', facecolor='#2d1b4e', alpha=0.7))

# ==================== STAGE 3 ====================
draw_stage_header(ax, 14.5, 3, 'STATISTICAL ANALYSIS', colors['stage3'])

box_stat = draw_box(ax, 2, 12, 4, 1.8, colors['stage3'],
                    'Bigram Model Scoring', 'Cosine similarity\nPre-trained models')

stat_details = ['65536 bigram entries', 'Weighted profile building', 'Language-specific models']
for i, detail in enumerate(stat_details):
    ax.text(3, 12.3 - i*0.25, f'* {detail}', fontsize=6,
            color=colors['subtext'], va='top')

box_simplified = draw_box(ax, 7, 12, 3.5, 1.8, colors['stage3'],
                          'Fallback Scoring', 'Entropy-based\nPattern matching')

box_confusion = draw_box(ax, 11.5, 12, 4, 1.8, colors['stage3'],
                         'Confusion Resolution', 'Similar encoding\nTie-breaking')

confusion_text = 'Common pairs:\nISO-8859-1 <-> Win-1252\nISO-8859-2 <-> Win-1250\nKOI8-R <-> Win-1251'
ax.text(16.5, 12.9, confusion_text, ha='center', va='center',
        fontsize=6, color=colors['text'], family='monospace',
        bbox=dict(boxstyle='round', facecolor='#1a1a3e', alpha=0.7))

# ==================== OUTPUT ====================
output_box = FancyBboxPatch((6, 9.5), 8, 1.8,
                            boxstyle="round,pad=0.02,rounding_size=0.3",
                            facecolor=colors['success'], edgecolor='white',
                            linewidth=3, alpha=0.3, zorder=2)
ax.add_patch(output_box)

ax.text(10, 10.8, 'OUTPUT: DetectionResult', ha='center', va='top',
        fontsize=14, fontweight='bold', color=colors['success'])
ax.text(10, 10.3, '{ encoding, confidence, language }', ha='center', va='top',
        fontsize=10, color=colors['text'], family='monospace')

ax.text(10, 9.8, 'Confidence Scale:', ha='center', va='top',
        fontsize=9, color=colors['subtext'])

# Confidence bar
confidence_points = [
    (0.0, 0.2, '#ff4444', 'Low'),
    (0.2, 0.5, '#ffaa44', 'Medium'),
    (0.5, 0.8, '#44aaff', 'Good'),
    (0.8, 0.95, '#44ff88', 'High'),
    (0.95, 1.0, '#00ffff', 'Certain')
]

bar_y = 9.2
for start, end, color, label in confidence_points:
    width = (end - start) * 12
    x_pos = 4 + start * 12
    rect = plt.Rectangle((x_pos, bar_y), width, 0.3,
                         facecolor=color, edgecolor='white',
                         linewidth=1, alpha=0.8)
    ax.add_patch(rect)
    ax.text(x_pos + width/2, bar_y + 0.15, label, ha='center', va='center',
            fontsize=6, color='white', fontweight='bold')

ax.text(4, bar_y - 0.2, '0.0', ha='center', fontsize=7, color=colors['subtext'])
ax.text(16, bar_y - 0.2, '1.0', ha='center', fontsize=7, color=colors['subtext'])

# ==================== ARCHITECTURE INFO ====================
info_x = 1
info_y = 7

ax.text(info_x, info_y, 'PIPELINE CHARACTERISTICS', ha='left', va='top',
        fontsize=11, fontweight='bold', color=colors['header'])

characteristics = [
    ('Early Exit', 'Deterministic stages can return immediately'),
    ('Progressive', 'Cheap checks before expensive analysis'),
    ('Caching', 'PipelineContext avoids redundant work'),
    ('Bounded', 'Default 200KB analysis limit'),
    ('Streaming', 'Supports incremental feeding')
]

for i, (title, desc) in enumerate(characteristics):
    y_pos = info_y - 0.5 - i*0.6
    ax.text(info_x, y_pos, f'> {title}:', ha='left', va='top',
            fontsize=8, color=colors['text'], fontweight='bold')
    ax.text(info_x + 0.3, y_pos - 0.25, desc, ha='left', va='top',
            fontsize=7, color=colors['subtext'])

# Performance metrics
perf_x = 13
perf_y = 7

ax.text(perf_x, perf_y, 'PERFORMANCE METRICS', ha='left', va='top',
        fontsize=11, fontweight='bold', color=colors['header'])

metrics = [
    ('BOM Detection', 'O(1)', 'Constant time'),
    ('ASCII Check', 'O(n)', 'Single pass'),
    ('UTF-8 Valid', 'O(n)', 'Single pass'),
    ('Structural', 'O(n*m)', 'Per encoding'),
    ('Statistical', 'O(n+m)', 'Bigram scoring')
]

for i, (stage, complexity, note) in enumerate(metrics):
    y_pos = perf_y - 0.5 - i*0.6
    ax.text(perf_x, y_pos, f'{stage}:', ha='left', va='top',
            fontsize=8, color=colors['text'])
    ax.text(perf_x + 2.5, y_pos, complexity, ha='left', va='top',
            fontsize=8, color=colors['success'], family='monospace')
    ax.text(perf_x + 3.5, y_pos, f'({note})', ha='left', va='top',
            fontsize=6, color=colors['subtext'])

# ==================== DATA FLOW ARROWS ====================
arrow_style = dict(arrowstyle='->', color=colors['arrow'], lw=2, mutation_scale=15)

stages_y = [24.5, 23, 20, 17, 14, 11.3]
for i in range(len(stages_y) - 1):
    ax.annotate('', xy=(10, stages_y[i+1]), xytext=(10, stages_y[i]),
                arrowprops=arrow_style)

exit_style = dict(arrowstyle='->', color=colors['success'], lw=1.5, 
                  mutation_scale=12, linestyle='--')
for x in [3.75, 7.75, 11.75, 16]:
    ax.annotate('', xy=(19, 22.5), xytext=(x + 1.5 if x < 15 else x + 2, 22.25),
                arrowprops=exit_style)

ax.annotate('', xy=(19, 19.5), xytext=(5.5, 19.25), arrowprops=exit_style)
ax.annotate('', xy=(19, 19.5), xytext=(9.25, 19.25), arrowprops=exit_style)

# ==================== LEGEND ====================
legend_y = 2.5
ax.text(1, legend_y, 'LEGEND:', ha='left', va='top',
        fontsize=10, fontweight='bold', color=colors['text'])

legend_items = [
    (colors['stage0'], 'Early Detection (Deterministic)'),
    (colors['stage1'], 'Basic Text Analysis'),
    (colors['stage2'], 'Structural Analysis'),
    (colors['stage3'], 'Statistical Analysis'),
    (colors['success'], 'High Confidence Exit'),
    (colors['binary'], 'Binary Detection'),
]

for i, (color, label) in enumerate(legend_items):
    x_pos = 1 + (i % 3) * 6
    y_pos = legend_y - 0.5 - (i // 3) * 0.5
    rect = plt.Rectangle((x_pos, y_pos - 0.1), 0.4, 0.25, 
                         facecolor=color, edgecolor='white', linewidth=1)
    ax.add_patch(rect)
    ax.text(x_pos + 0.6, y_pos, label, ha='left', va='center',
            fontsize=8, color=colors['text'])

# ==================== FOOTER ====================
ax.text(10, 0.5, 'Chardet-Rust - Universal Character Encoding Detection - Rust + PyO3',
        ha='center', va='center', fontsize=9, color=colors['subtext'], style='italic')

plt.tight_layout()
plt.savefig('docs/pipeline_infographic.png', dpi=150, bbox_inches='tight',
            facecolor=fig.get_facecolor(), edgecolor='none')
plt.savefig('docs/pipeline_infographic.svg', bbox_inches='tight',
            facecolor=fig.get_facecolor(), edgecolor='none')
print("Infographic saved to docs/pipeline_infographic.png and .svg")

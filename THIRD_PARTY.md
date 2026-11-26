# Third-party code and assets

Sketchforge is MIT licensed. It ports algorithms from MIT-licensed projects and selected
draw.io logic from Apache-2.0 sources, listed below with copyright notices and license text.
It bundles the Inter font (SIL OFL 1.1, `assets/fonts/OFL.txt`) and Noto Sans Arabic (SIL OFL
1.1, `assets/fonts/NotoSansArabic-NOTICE.txt`). Figma and Paper are proprietary: only their
*behavior* was studied (`analysis/FIGMA_NOTES.md`, `analysis/PAPER_ARCHITECTURE.md`), and no
code, shaders, icons or assets from them are used.

## Ported code

| Project | Copyright | What was ported | Where |
|---|---|---|---|
| [rough.js](https://github.com/rough-stuff/rough) 4.6.4 | Copyright (c) 2019 Preet Shihn | Seeded RNG, `_line` / `_doubleLine`, curves, ellipses, `svgPath`, solid / hachure / cross-hatch / zigzag / dots fills, generator option handling. Verified against rough.js output (`tests/fixtures/rough/`). | `src/render/rough.rs` |
| [hachure-fill](https://github.com/pshihn/hachure-fill) 0.5.2 | Copyright (c) 2023 Preet Shihn | Scan-line hachure algorithm | `src/render/rough.rs` |
| [points-on-curve](https://github.com/pshihn/bezier-points) 0.2.0, [points-on-path](https://github.com/pshihn/points-on-path) 0.2.1 | Copyright (c) 2020 Preet Shihn | Bezier flattening by flatness, Ramer–Douglas–Peucker, `curveToBezier`, path point sets | `src/render/rough.rs` |
| [Excalidraw](https://github.com/excalidraw/excalidraw) | Copyright (c) 2020 Excalidraw | Element → rough options (`adjustRoughness`, dash patterns, fill weights), binding gap and fixed-point rules, elbow-arrow grid/A* router with its segment-count heuristic, elbow corner rounding, arrowhead geometry, laser trail decay constants, sticky-note sizing, `.excalidraw` / `.excalidrawlib` formats | `src/render/rough.rs`, `src/connector.rs`, `src/editor/whiteboard.rs`, `src/io/excalidraw.rs` |
| [draw.io](https://github.com/jgraph/drawio) | Copyright (c) jGraph Holdings Ltd / draw.io AG | Color role defaults, `default<Key>=invert`, CSS color parsing, `light-dark()` pairs, draw.io style parsing for data URIs and gradients, swimlane fill selection, native color export mapping, BPMN shape aliases (`mxgraph.bpmn.event`, `mxgraph.bpmn.task2`, `mxgraph.bpmn.gateway2`) with their outline types and export styles, C4 shape aliases with their `c4Type` property and palette styles, SysML port aliases with the `<<block>>` and `<<requirement>>` composite markers, flowchart stencil aliases such as `document2`, `card`, `multi-document`, `manual_input`, `on-page_reference`, `direct_data`, `merge_or_storage`, `tape`, `or_2`, `summing_junction`, `start_1`, `start_2`, `mxgraph.flowchart.process`, `internalStorage`, `cylinder3`, `shape=hexagon;`, `shape=mxgraph.flowchart.terminator;`, `shape=mxgraph.flowchart.decision;`, `shape=mxgraph.flowchart.delay;`, `shape=mxgraph.flowchart.document2;` and the Data palette perimeter and size flags, the Manual Input palette label and arc flags, the Manual Operation trapezoid and label flags, and the Preparation hexagon perimeter and size flags, the Off-Page connector size and label flags, the Process rounded rectangle flags, the Document stencil size flag, and the Terminator stroke width flag, and the Decision stroke width flag, and the Delay stroke width flag, and the Database flowchart stencil flag, the On-Page Reference palette flag, the UML Actor wrap flag, the network palette's lack of a wrap flag and the Cloud label font color, plus Chen ER style flags for weak entities, key attributes and derived attributes, ER cardinality markers with preserved draw.io names, plus UML note, package and actor aliases such as `shape=note2;`, `shape=folder;` and `shape=umlActor;`, composite class children, floorplan stencil aliases such as `mxgraph.floorplan.doorLeft`, network stencil aliases such as `mxgraph.networks.mail_server`, the mermaid person stencil, the mermaid use case actor variants, and the basic pie stencil with fractional angles and its palette style, and the basic Star stencil alias and palette label flags, plus the four, six and eight point star aliases and their export styles, and the basic Pentagon and Octagon stencils with their export styles, plus the Basic Orthogonal, Acute and Obtuse Triangle aliases with their reported approximations. Sources include `src/main/webapp/styles/default.xml`, `js/grapheditor/Editor.js`, `js/grapheditor/Graph.js`, `mxgraph/src/util/mxUtils.js`, `mxgraph/src/shape/mxShape.js`, `js/diagramly/sidebar/Sidebar-BPMN.js`, `shapes/bpmn/mxBpmnShape2.js`, `js/diagramly/sidebar/Sidebar-C4.js`, `js/diagramly/sidebar/Sidebar-Sysml.js`, `shapes/mxSysML.js`, `js/diagramly/sidebar/Sidebar-Flowchart.js`, `js/diagramly/sidebar/Sidebar-ER.js`, `shapes/er/mxER.js`, `js/diagramly/sidebar/Sidebar-UML25.js`, `js/diagramly/sidebar/Sidebar-Floorplan.js`, `js/diagramly/sidebar/Sidebar-Network.js`, `js/diagramly/sidebar/Sidebar-Basic.js` and `shapes/mxBasic.js`. | `src/io/drawio.rs` |

Each ported module starts with a comment naming its source.

## Dependencies

Rust crates are pulled from crates.io under their own licenses (see `Cargo.lock`); notable ones
are wgpu, winit, egui, lyon, ttf-parser, image, i_overlay (MIT), serde and rfd.

The macOS release embeds [Sparkle](https://sparkle-project.org) 2.10.0, unmodified, to install
updates. Sparkle is MIT-licensed, copyright 2006-2013 Andy Matuschak and others, and bundles code
under the other licenses in its `LICENSE`. `scripts/release-macos.sh` copies that file into the
app as `Contents/Resources/Sparkle-LICENSE.txt`.

## macOS release and updates

`scripts/release-macos.sh`, `scripts/fetch-sparkle.sh` and `src/macos/updater.rs` are ported
from [terminal-rs](https://github.com/michaelshimeles/terminal-rs) and
[glide](https://github.com/michaelshimeles/glide), other projects by the same author. Each starts
with a comment naming its source.

## Apache License 2.0 (applies to the draw.io / jGraph port)

```text
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

## MIT License (applies to each MIT project listed above, with its copyright line)

```
MIT License

Copyright (c) 2019 Preet Shihn            (rough.js)
Copyright (c) 2020 Preet Shihn            (points-on-curve, points-on-path, path-data-parser)
Copyright (c) 2023 Preet Shihn            (hachure-fill)
Copyright (c) 2020 Excalidraw             (Excalidraw)
Copyright (c) 2023 Excalidraw             (@excalidraw/laser-pointer)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Download page

`site/` follows the download page of [terminal-rs](https://github.com/michaelshimeles/terminal-rs)
(`site/`), another project by the same author, ported at his request. The page layout and base
styles, the demo's player, desktop and sound effects, the easing, frame, motion and theme helpers,
the card scaler and the Open Graph layout come from there, and each of those files starts with a
comment naming its source. The page also
bundles [rough.js](https://github.com/rough-stuff/rough) 4.6.6 from npm (MIT, Copyright (c) 2019
Preet Shihn, license text above) to draw its hand-drawn shapes.

## Agent skills

`.agents/skills/` holds instructions and helper scripts for coding agents, installed from
[michaelshimeles/skills](https://github.com/michaelshimeles/skills). They are not part of the
app. The ones vendored from other projects keep their license file in their folder:

| Skill | From | License |
|---|---|---|
| `before-and-after` | [vercel-labs/before-and-after](https://github.com/vercel-labs/before-and-after), Copyright (c) 2026 James Clements | PolyForm Shield 1.0.0 |
| `greploop`, `greploop-apps` | [greptileai/skills](https://github.com/greptileai/skills), Copyright (c) 2026 Greptile AI | MIT |
| `unslop` | [cursor/plugins](https://github.com/cursor/plugins/tree/main/pstack/skills/unslop), Copyright (c) 2026 Lauren Tan | MIT |

## Fonts

Inter 4.1 by Rasmus Andersson — SIL Open Font License 1.1 (`assets/fonts/OFL.txt`). Regular,
Medium, SemiBold, Bold, Italic and Bold Italic are bundled unmodified.

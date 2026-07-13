using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using DisplayAudioOrchestrator.CCD;
using DisplayAudioOrchestrator.Orchestrator;
using SetResolutionAdapters;

namespace DisplayAudioOrchestrator.GUI
{
    public sealed class ProfileEditorForm : Form
    {
        public enum EditorAction { Saved, Deleted, Cancelled }

        public EditorAction        Action        { get; private set; } = EditorAction.Cancelled;
        public string              NewName       { get; private set; }
        public OrchestratorProfile EditedProfile { get; private set; }

        private readonly string _originalName;
        private bool _suppressEvents;
        private int  _currentDisplayIndex = -1;

        private readonly List<ProfileDisplay>             _displayList;
        private readonly Dictionary<string, List<DisplayModeInfo>> _modeCache;

        private TextBox      _txtName;
        private ListBox      _lstDisplays;
        private Panel        _pnlDetail;
        private ComboBox     _cmbGdi, _cmbActive, _cmbPrimary, _cmbResolution, _cmbHz, _cmbDpi, _cmbHdr;
        private DataGridView _dgvAudio;
        private DataGridView _dgvProcesses;

        private static readonly string[] BoolItems = { "", "Yes", "No" };

        public ProfileEditorForm(string profileName, OrchestratorProfile profile)
        {
            _originalName = profileName;
            _displayList  = new List<ProfileDisplay>(profile.Displays ?? new List<ProfileDisplay>());
            _modeCache    = new Dictionary<string, List<DisplayModeInfo>>(StringComparer.OrdinalIgnoreCase);
            InitComponents();
            _txtName.Text = profileName;
            RefreshDisplayList();
            if (_displayList.Count > 0) SelectDisplay(0);
            PopulateAudio(profile.Audio ?? new List<ProfileAudio>());
            PopulateProcesses(profile.StartProcesses ?? new List<StartProcess>());
        }

        // ── Init ──────────────────────────────────────────────────────────────

        private void InitComponents()
        {
            Text            = "Edit Profile";
            ClientSize      = new Size(860, 580);
            FormBorderStyle = FormBorderStyle.Sizable;
            StartPosition   = FormStartPosition.CenterParent;
            MinimumSize     = new Size(680, 440);

            var pnlName = new Panel { Height = 34 };
            Controls.Add(pnlName);
            pnlName.Controls.Add(new Label { Text = "Profile name:", Left = 8, Top = 8, Width = 110, Height = 20 });
            _txtName = new TextBox { Left = 122, Top = 6, Width = 400 };
            pnlName.Controls.Add(_txtName);

            var tabs = new TabControl();
            Controls.Add(tabs);
            var tabD = new TabPage("Displays");
            var tabA = new TabPage("Audio");
            var tabP = new TabPage("Processes");
            tabs.TabPages.AddRange(new TabPage[] { tabD, tabA, tabP });

            InitDisplaysTab(tabD);

            _dgvAudio = CreateGrid();
            _dgvAudio.Columns.Add(TxtCol("Pattern", "Pattern (FriendlyName)", 260, true));
            _dgvAudio.Columns.Add(TypeCombo());
            _dgvAudio.Columns.Add(BoolCombo("SetDefault", "Default", 65));
            _dgvAudio.Columns.Add(TxtCol("Volume", "Vol%", 52));
            _dgvAudio.Columns.Add(BoolCombo("Mute", "Mute", 55));
            AddGridWithButtons(tabA, _dgvAudio);

            _dgvProcesses = CreateGrid();
            _dgvProcesses.Columns.Add(TxtCol("Path", "Path", 340, true));
            _dgvProcesses.Columns.Add(TxtCol("Args", "Args", 200));
            _dgvProcesses.Columns.Add(BoolCombo("AsAdmin", "As Admin", 72));
            AddGridWithButtons(tabP, _dgvProcesses);

            var pnlBtns = new Panel { Height = 46 };
            Controls.Add(pnlBtns);
            var btnSave   = new Button { Text = "Save",          Left = 0,  Top = 8, Width = 90,  Height = 30 };
            var btnDelete = new Button { Text = "Delete Profile", Left = 98, Top = 8, Width = 120, Height = 30, ForeColor = Color.DarkRed };
            var btnCancel = new Button { Text = "Cancel",                   Top = 8, Width = 80,  Height = 30 };
            btnSave.Click   += BtnSave_Click;
            btnDelete.Click += BtnDelete_Click;
            btnCancel.Click += (s, e) => Close();
            pnlBtns.Controls.AddRange(new Control[] { btnSave, btnDelete, btnCancel });

            Resize += (s, e) => DoLayout();
            Load   += (s, e) => DoLayout();
        }

        private void InitDisplaysTab(TabPage tab)
        {
            var split = new SplitContainer
            {
                Dock = DockStyle.Fill, Orientation = Orientation.Vertical,
                SplitterDistance = 195, SplitterWidth = 5
            };
            tab.Controls.Add(split);

            // Left: list + buttons
            _lstDisplays = new ListBox { Dock = DockStyle.Fill, Font = new Font("Segoe UI", 9f) };
            _lstDisplays.SelectedIndexChanged += LstDisplays_SelectionChanged;
            split.Panel1.Controls.Add(_lstDisplays);

            var pnlListBtns = new Panel { Dock = DockStyle.Bottom, Height = 36 };
            var btnAdd = new Button { Text = "Add",    Left = 4,  Top = 4, Width = 60, Height = 28 };
            var btnRem = new Button { Text = "Remove", Left = 68, Top = 4, Width = 70, Height = 28 };
            btnAdd.Click += BtnAddDisplay_Click;
            btnRem.Click += BtnRemoveDisplay_Click;
            pnlListBtns.Controls.AddRange(new Control[] { btnAdd, btnRem });
            split.Panel1.Controls.Add(pnlListBtns);

            // Right: detail fields
            _pnlDetail = new Panel { Dock = DockStyle.Fill };
            split.Panel2.Controls.Add(_pnlDetail);

            int y = 10;
            _cmbGdi     = DetailRow("GDI Name:", ref y, ComboBoxStyle.DropDown);
            _cmbActive  = DetailRow("Active:",   ref y, items: BoolItems);
            _cmbPrimary = DetailRow("Primary:",  ref y, items: BoolItems);
            DetailSep(ref y);
            _cmbResolution = DetailRow("Resolution:", ref y, ComboBoxStyle.DropDown);
            _cmbHz         = DetailRow("Hz:",         ref y, ComboBoxStyle.DropDown);
            DetailSep(ref y);

            uint[] dpiVals = DisplayConfigFlags.DpiValues;
            var dpiItems = new string[dpiVals.Length + 1];
            dpiItems[0] = "";
            for (int i = 0; i < dpiVals.Length; i++) dpiItems[i + 1] = dpiVals[i].ToString();
            _cmbDpi = DetailRow("DPI%:", ref y, ComboBoxStyle.DropDown, dpiItems);
            _cmbHdr = DetailRow("HDR:",  ref y, items: BoolItems);

            _cmbGdi.DropDown += CmbGdi_DropDown;
            _cmbGdi.Leave    += (s, e) => OnGdiChanged();
            _cmbGdi.KeyDown  += (s, e) => { if (e.KeyCode == Keys.Enter) OnGdiChanged(); };

            _cmbResolution.SelectedIndexChanged += CmbResolution_Changed;
            _cmbResolution.Leave += (s, e) => { if (!_suppressEvents) CmbResolution_Changed(s, e); };

            foreach (ComboBox c in new[] { _cmbActive, _cmbPrimary, _cmbHz, _cmbDpi, _cmbHdr })
                c.SelectedIndexChanged += (s, e) => { if (!_suppressEvents) SaveDetailToList(); };
            _cmbHz.Leave  += (s, e) => { if (!_suppressEvents) SaveDetailToList(); };
            _cmbDpi.Leave += (s, e) => { if (!_suppressEvents) SaveDetailToList(); };

            _pnlDetail.Enabled = false;
        }

        private ComboBox DetailRow(string label, ref int y,
            ComboBoxStyle style = ComboBoxStyle.DropDownList, string[] items = null)
        {
            const int lblW = 85, ctrlX = 98, rowH = 26, gap = 6;
            _pnlDetail.Controls.Add(new Label { Text = label, Left = 8, Top = y + 3, Width = lblW, Height = 20 });
            var c = new ComboBox { Left = ctrlX, Top = y, Width = 260, Height = rowH, DropDownStyle = style, Tag = "dc" };
            if (items != null) c.Items.AddRange(items);
            _pnlDetail.Controls.Add(c);
            y += rowH + gap;
            return c;
        }

        private void DetailSep(ref int y)
        {
            y += 4;
            _pnlDetail.Controls.Add(new Label { Left = 8, Top = y, Width = 300, Height = 1, BackColor = Color.Silver, Tag = "ds" });
            y += 10;
        }

        // ── Layout ────────────────────────────────────────────────────────────

        private void DoLayout()
        {
            const int pad = 8, nameH = 34, btnH = 46;
            int w = ClientSize.Width, h = ClientSize.Height;

            if (Controls.Count >= 1 && Controls[0] is Panel pn)
            {
                pn.SetBounds(pad, pad, w - pad * 2, nameH);
                if (pn.Controls.Count >= 2) pn.Controls[1].Width = Math.Min(400, pn.Width - 130);
            }
            if (Controls.Count >= 2 && Controls[1] is TabControl tc)
                tc.SetBounds(pad, nameH + pad * 2, w - pad * 2, h - nameH - btnH - pad * 4);
            if (Controls.Count >= 3 && Controls[2] is Panel pb)
            {
                pb.SetBounds(pad, h - btnH - pad, w - pad * 2, btnH);
                if (pb.Controls.Count >= 3) pb.Controls[2].Left = pb.Width - 84;
            }
            LayoutDetailPanel();
        }

        private void LayoutDetailPanel()
        {
            if (_pnlDetail == null) return;
            int pw = _pnlDetail.ClientSize.Width;
            if (pw < 40) return;
            int ctrlW = Math.Max(60, pw - 98 - 12);
            foreach (Control c in _pnlDetail.Controls)
            {
                if      ("dc".Equals(c.Tag as string) && c is ComboBox) c.Width = ctrlW;
                else if ("ds".Equals(c.Tag as string) && c is Label)    c.Width = pw - 16;
            }
        }

        // ── Display list management ───────────────────────────────────────────

        private void RefreshDisplayList()
        {
            _suppressEvents = true;
            _lstDisplays.BeginUpdate();
            _lstDisplays.Items.Clear();
            foreach (var d in _displayList)
            {
                string gdi = string.IsNullOrEmpty(d.GdiName) ? "(new)" : d.GdiName;
                string res = d.Width != null && d.Height != null
                    ? "  " + d.Width + "×" + d.Height + (d.Hz != null ? "@" + d.Hz : "")
                    : "";
                _lstDisplays.Items.Add(gdi + res);
            }
            if (_currentDisplayIndex >= 0 && _currentDisplayIndex < _lstDisplays.Items.Count)
                _lstDisplays.SelectedIndex = _currentDisplayIndex;
            _lstDisplays.EndUpdate();
            _suppressEvents = false;
        }

        private void SelectDisplay(int idx)
        {
            if (idx == _currentDisplayIndex && idx >= 0) return;
            SaveDetailToList();
            _currentDisplayIndex = idx;

            if (idx < 0 || idx >= _displayList.Count)
            {
                _pnlDetail.Enabled = false;
                return;
            }
            _pnlDetail.Enabled = true;
            _suppressEvents = true;

            var d = _displayList[idx];
            _cmbGdi.Text = d.GdiName ?? string.Empty;
            SetBoolCmb(_cmbActive,  d.Active);
            SetBoolCmb(_cmbPrimary, d.Primary);
            SetBoolCmb(_cmbHdr,     d.Hdr);

            LoadResolutionCombo(d.GdiName);
            string selRes = d.Width != null && d.Height != null ? FmtRes(d.Width.Value, d.Height.Value) : string.Empty;
            SetTxt(_cmbResolution, selRes);
            LoadHzCombo(selRes);
            SetTxt(_cmbHz,  d.Hz?.ToString() ?? string.Empty);
            SetTxt(_cmbDpi, d.DpiPercent?.ToString() ?? string.Empty);

            _suppressEvents = false;
            _lstDisplays.SelectedIndex = idx;
        }

        private void SaveDetailToList()
        {
            if (_currentDisplayIndex < 0 || _currentDisplayIndex >= _displayList.Count) return;
            var d = _displayList[_currentDisplayIndex];
            d.GdiName    = _cmbGdi.Text.Trim();
            d.Active     = GetBoolCmb(_cmbActive);
            d.Primary    = GetBoolCmb(_cmbPrimary);
            d.Hdr        = GetBoolCmb(_cmbHdr);
            d.DpiPercent = ParseInt(_cmbDpi.Text);
            if (TryParseRes(_cmbResolution.Text, out int rw, out int rh)) { d.Width = rw; d.Height = rh; }
            else { d.Width = null; d.Height = null; }
            d.Hz = ParseInt(_cmbHz.Text);
        }

        private void LoadResolutionCombo(string gdiName)
        {
            _cmbResolution.Items.Clear();
            if (string.IsNullOrWhiteSpace(gdiName)) return;
            var seen = new SortedSet<string>(ResolutionComparer.Instance);
            foreach (var m in GetModes(gdiName))
                if (m.Width > 0 && m.Height > 0) seen.Add(FmtRes(m.Width, m.Height));
            foreach (var r in seen) _cmbResolution.Items.Add(r);
        }

        private void LoadHzCombo(string resText)
        {
            _cmbHz.Items.Clear();
            if (!TryParseRes(resText, out int rw, out int rh)) return;
            var seen = new SortedSet<int>(Comparer<int>.Create((a, b) => b.CompareTo(a)));
            foreach (var m in GetModes(_cmbGdi.Text.Trim()))
                if (m.Width == rw && m.Height == rh && m.Hz > 0) seen.Add(m.Hz);
            foreach (var hz in seen) _cmbHz.Items.Add(hz.ToString());
        }

        private List<DisplayModeInfo> GetModes(string gdi)
        {
            if (string.IsNullOrWhiteSpace(gdi)) return new List<DisplayModeInfo>();
            if (!_modeCache.ContainsKey(gdi))
            {
                List<DisplayModeInfo> m;
                try   { m = DisplayManagerAdapter.GetDisplayModes(gdi); }
                catch { m = new List<DisplayModeInfo>(); }
                _modeCache[gdi] = m;
            }
            return _modeCache[gdi];
        }

        private void OnGdiChanged()
        {
            if (_suppressEvents) return;
            SaveDetailToList();
            RefreshDisplayList();
            LoadResolutionCombo(_cmbGdi.Text.Trim());
        }

        // ── Events ────────────────────────────────────────────────────────────

        private void LstDisplays_SelectionChanged(object sender, EventArgs e)
        {
            if (_suppressEvents) return;
            int idx = _lstDisplays.SelectedIndex;
            if (idx != _currentDisplayIndex) SelectDisplay(idx);
        }

        private void CmbGdi_DropDown(object sender, EventArgs e)
        {
            if (_cmbGdi.Items.Count > 0) return;
            try
            {
                foreach (var dev in DisplayManagerAdapter.GetAllDisplayDevices())
                    _cmbGdi.Items.Add(dev.GdiName);
            }
            catch { }
        }

        private void CmbResolution_Changed(object sender, EventArgs e)
        {
            if (_suppressEvents) return;
            string prevHz = _cmbHz.Text;
            LoadHzCombo(_cmbResolution.Text);
            if (!string.IsNullOrEmpty(prevHz) && _cmbHz.Items.Contains(prevHz))
                SetTxt(_cmbHz, prevHz);
            else if (_cmbHz.Items.Count > 0)
                _cmbHz.SelectedIndex = 0;
            SaveDetailToList();
            RefreshDisplayList();
        }

        private void BtnAddDisplay_Click(object sender, EventArgs e)
        {
            SaveDetailToList();
            _displayList.Add(new ProfileDisplay { GdiName = "DISPLAY" + (_displayList.Count + 1) });
            _currentDisplayIndex = _displayList.Count - 1;
            RefreshDisplayList();
            SelectDisplay(_currentDisplayIndex);
        }

        private void BtnRemoveDisplay_Click(object sender, EventArgs e)
        {
            int idx = _currentDisplayIndex;
            if (idx < 0 || idx >= _displayList.Count) return;
            _displayList.RemoveAt(idx);
            _currentDisplayIndex = -1;
            RefreshDisplayList();
            int next = Math.Min(idx, _displayList.Count - 1);
            if (next >= 0) SelectDisplay(next);
            else _pnlDetail.Enabled = false;
        }

        // ── Save / Delete ─────────────────────────────────────────────────────

        private void BtnSave_Click(object sender, EventArgs e)
        {
            SaveDetailToList();
            string name = _txtName.Text.Trim();
            if (string.IsNullOrEmpty(name))
            {
                MessageBox.Show("Profile name cannot be empty.", "Validation", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            EditedProfile = new OrchestratorProfile
            {
                Displays       = new List<ProfileDisplay>(_displayList),
                Audio          = ReadAudio(),
                StartProcesses = ReadProcesses()
            };
            NewName = name;
            Action  = EditorAction.Saved;
            Close();
        }

        private void BtnDelete_Click(object sender, EventArgs e)
        {
            if (MessageBox.Show($"Permanently delete profile '{_originalName}'?", "Confirm Delete",
                    MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) return;
            Action = EditorAction.Deleted;
            Close();
        }

        // ── Audio / Process grids ─────────────────────────────────────────────

        private void PopulateAudio(List<ProfileAudio> items)
        {
            foreach (var a in items)
                _dgvAudio.Rows.Add(a.Pattern ?? string.Empty, a.Type ?? "Playback",
                    BoolStr(a.SetDefault), IntStr(a.Volume), BoolStr(a.Mute));
        }

        private void PopulateProcesses(List<StartProcess> items)
        {
            foreach (var p in items)
                _dgvProcesses.Rows.Add(p.Path ?? string.Empty, p.Args ?? string.Empty, BoolStr(p.AsAdmin));
        }

        private List<ProfileAudio> ReadAudio()
        {
            var list = new List<ProfileAudio>();
            foreach (DataGridViewRow row in _dgvAudio.Rows)
            {
                string pattern = CellStr(row, 0);
                if (string.IsNullOrWhiteSpace(pattern)) continue;
                string type = CellStr(row, 1);
                list.Add(new ProfileAudio
                {
                    Pattern    = pattern.Trim(),
                    Type       = string.IsNullOrWhiteSpace(type) ? "Playback" : type,
                    SetDefault = ParseBool(CellStr(row, 2)),
                    Volume     = ParseInt(CellStr(row, 3)),
                    Mute       = ParseBool(CellStr(row, 4))
                });
            }
            return list;
        }

        private List<StartProcess> ReadProcesses()
        {
            var list = new List<StartProcess>();
            foreach (DataGridViewRow row in _dgvProcesses.Rows)
            {
                string path = CellStr(row, 0);
                if (string.IsNullOrWhiteSpace(path)) continue;
                list.Add(new StartProcess
                {
                    Path    = path.Trim(),
                    Args    = CellStr(row, 1),
                    AsAdmin = ParseBool(CellStr(row, 2)) == true
                });
            }
            return list;
        }

        // ── Grid factory ──────────────────────────────────────────────────────

        private static DataGridView CreateGrid()
        {
            var dgv = new DataGridView
            {
                Dock                  = DockStyle.Fill,
                AllowUserToAddRows    = false,
                AllowUserToDeleteRows = false,
                SelectionMode         = DataGridViewSelectionMode.FullRowSelect,
                MultiSelect           = true,
                RowHeadersWidth       = 24,
                BackgroundColor       = SystemColors.Window,
                GridColor             = SystemColors.ControlLight,
                ColumnHeadersHeightSizeMode = DataGridViewColumnHeadersHeightSizeMode.DisableResizing,
                ColumnHeadersHeight   = 24
            };
            dgv.DataError += (s, e) => e.Cancel = true;
            return dgv;
        }

        private static void AddGridWithButtons(TabPage tab, DataGridView dgv)
        {
            var pnl = new Panel { Dock = DockStyle.Bottom, Height = 36 };
            tab.Controls.Add(pnl);
            tab.Controls.Add(dgv);
            var btnAdd = new Button { Text = "Add Row",         Left = 4,  Top = 4, Width = 80,  Height = 28 };
            var btnRem = new Button { Text = "Remove Selected", Left = 88, Top = 4, Width = 130, Height = 28 };
            btnAdd.Click += (s, e) => dgv.Rows.Add();
            btnRem.Click += (s, e) =>
            {
                var idxs = new List<int>();
                foreach (DataGridViewRow r in dgv.SelectedRows) if (!r.IsNewRow) idxs.Add(r.Index);
                idxs.Sort((a, b) => b.CompareTo(a));
                foreach (int i in idxs) dgv.Rows.RemoveAt(i);
            };
            pnl.Controls.AddRange(new Control[] { btnAdd, btnRem });
        }

        private static DataGridViewTextBoxColumn TxtCol(string name, string header, int width, bool fill = false)
        {
            var col = new DataGridViewTextBoxColumn { Name = name, HeaderText = header, Width = width };
            if (fill) col.AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill;
            return col;
        }

        private static DataGridViewComboBoxColumn BoolCombo(string name, string header, int width)
        {
            var col = new DataGridViewComboBoxColumn { Name = name, HeaderText = header, Width = width, FlatStyle = FlatStyle.Flat };
            col.Items.AddRange(BoolItems);
            return col;
        }

        private static DataGridViewComboBoxColumn TypeCombo()
        {
            var col = new DataGridViewComboBoxColumn { Name = "Type", HeaderText = "Type", Width = 90, FlatStyle = FlatStyle.Flat };
            col.Items.AddRange(new object[] { "Playback", "Recording" });
            return col;
        }

        // ── Value helpers ─────────────────────────────────────────────────────

        private static string FmtRes(int w, int h) => w + " × " + h;

        private static bool TryParseRes(string s, out int w, out int h)
        {
            w = h = 0;
            if (string.IsNullOrWhiteSpace(s)) return false;
            s = s.Replace("×", "x").Replace("X", "x").Replace(" ", "");
            int ix = s.IndexOf('x');
            return ix > 0 && ix < s.Length - 1
                && int.TryParse(s.Substring(0, ix), out w)
                && int.TryParse(s.Substring(ix + 1), out h);
        }

        private static void SetTxt(ComboBox c, string text)
        {
            if (c.DropDownStyle == ComboBoxStyle.DropDownList)
            {
                int idx = c.Items.IndexOf(text ?? string.Empty);
                c.SelectedIndex = idx >= 0 ? idx : 0;
            }
            else
                c.Text = text ?? string.Empty;
        }

        private static void SetBoolCmb(ComboBox c, bool? v)
        {
            int idx = v == null ? 0 : v.Value ? 1 : 2;
            if (idx < c.Items.Count) c.SelectedIndex = idx;
        }

        private static bool? GetBoolCmb(ComboBox c)
        {
            if (c.SelectedIndex == 1) return true;
            if (c.SelectedIndex == 2) return false;
            return null;
        }

        private static string CellStr(DataGridViewRow row, int col)
        {
            object v = row.Cells[col].Value;
            return v == null ? string.Empty : v.ToString();
        }

        private static string BoolStr(bool? v) => v == null ? string.Empty : v.Value ? "Yes" : "No";
        private static string IntStr(int?  v)  => v == null ? string.Empty : v.ToString();

        private static bool? ParseBool(string s)
        {
            if (string.IsNullOrEmpty(s)) return null;
            if (s.Equals("Yes",  StringComparison.OrdinalIgnoreCase) ||
                s.Equals("true", StringComparison.OrdinalIgnoreCase) || s == "1") return true;
            if (s.Equals("No",   StringComparison.OrdinalIgnoreCase) ||
                s.Equals("false",StringComparison.OrdinalIgnoreCase) || s == "0") return false;
            return null;
        }

        private static int? ParseInt(string s) =>
            string.IsNullOrWhiteSpace(s) ? (int?)null
            : int.TryParse(s.Trim(), out int v) ? (int?)v : null;

        // ── Resolution sort (highest pixel count first) ───────────────────────

        private sealed class ResolutionComparer : IComparer<string>
        {
            public static readonly ResolutionComparer Instance = new ResolutionComparer();

            public int Compare(string x, string y)
            {
                TryParseRes(x, out int wx, out int hx);
                TryParseRes(y, out int wy, out int hy);
                long px = (long)wx * hx, py = (long)wy * hy;
                return px != py ? py.CompareTo(px) : string.Compare(x, y, StringComparison.Ordinal);
            }
        }
    }
}

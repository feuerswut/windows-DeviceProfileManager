using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using DisplayAudioOrchestrator.Audio;
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
        private readonly List<DisplayDeviceInfo> _displayDevices;
        private readonly List<AudioDeviceInfo>   _audioDevices;
        private readonly Dictionary<string, List<DisplayModeInfo>> _modeCache =
            new Dictionary<string, List<DisplayModeInfo>>(StringComparer.OrdinalIgnoreCase);

        private TextBox      _txtName;
        private DataGridView _dgvDisplays;
        private DataGridView _dgvAudio;
        private DataGridView _dgvProcesses;

        private static readonly string[] BoolItems = { "", "Yes", "No" };

        public ProfileEditorForm(string profileName, OrchestratorProfile profile)
        {
            _originalName   = profileName;
            _displayDevices = LoadDisplayDevices();
            _audioDevices   = LoadAudioDevices();
            InitComponents();
            _txtName.Text = profileName;
            PopulateDisplays(profile.Displays       ?? new List<ProfileDisplay>());
            PopulateAudio(profile.Audio             ?? new List<ProfileAudio>());
            PopulateProcesses(profile.StartProcesses ?? new List<StartProcess>());
        }

        // ── Device loading ────────────────────────────────────────────────────

        private static List<DisplayDeviceInfo> LoadDisplayDevices()
        {
            try   { return DisplayManagerAdapter.GetAllDisplayDevices(); }
            catch { return new List<DisplayDeviceInfo>(); }
        }

        private static List<AudioDeviceInfo> LoadAudioDevices()
        {
            try   { return AudioManager.GetAllDevices().Where(d => d.State == 1).ToList(); }
            catch { return new List<AudioDeviceInfo>(); }
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

            // Displays tab — monitor picker + dynamic resolution/hz combos
            _dgvDisplays = CreateGrid();
            _dgvDisplays.Columns.Add(MonitorCombo());
            _dgvDisplays.Columns.Add(BoolCombo("Active",  "Active",  60));
            _dgvDisplays.Columns.Add(BoolCombo("Primary", "Primary", 62));
            _dgvDisplays.Columns.Add(DynCombo("Resolution", "Resolution", 115));
            _dgvDisplays.Columns.Add(DynCombo("Hz", "Hz", 55));
            _dgvDisplays.Columns.Add(TxtCol("DpiPercent", "DPI%", 52));
            _dgvDisplays.Columns.Add(BoolCombo("Hdr", "HDR", 52));
            _dgvDisplays.EditingControlShowing        += DgvDisplays_EditingControlShowing;
            _dgvDisplays.CurrentCellDirtyStateChanged += (s, e) =>
            {
                if (_dgvDisplays.IsCurrentCellDirty && _dgvDisplays.CurrentCell?.ColumnIndex == 0)
                    _dgvDisplays.CommitEdit(DataGridViewDataErrorContexts.Commit);
            };
            AddGridWithButtons(tabD, _dgvDisplays);

            // Audio tab — device picker, type auto-fills
            _dgvAudio = CreateGrid();
            _dgvAudio.Columns.Add(AudioDeviceCombo());
            _dgvAudio.Columns.Add(TypeCombo());
            _dgvAudio.Columns.Add(BoolCombo("SetDefault", "Default", 65));
            _dgvAudio.Columns.Add(TxtCol("Volume", "Vol%", 52));
            _dgvAudio.Columns.Add(BoolCombo("Mute", "Mute", 55));
            _dgvAudio.CurrentCellDirtyStateChanged += (s, e) =>
            {
                if (_dgvAudio.IsCurrentCellDirty && _dgvAudio.CurrentCell?.ColumnIndex == 0)
                    _dgvAudio.CommitEdit(DataGridViewDataErrorContexts.Commit);
            };
            _dgvAudio.CellValueChanged += DgvAudio_DeviceChanged;
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

        // ── Column factories ──────────────────────────────────────────────────

        private DataGridViewComboBoxColumn MonitorCombo()
        {
            var col = new DataGridViewComboBoxColumn
            {
                Name = "Monitor", HeaderText = "Monitor",
                FlatStyle = FlatStyle.Flat,
                AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill
            };
            foreach (var dev in _displayDevices)
                col.Items.Add(FormatMonitor(dev));
            return col;
        }

        private DataGridViewComboBoxColumn AudioDeviceCombo()
        {
            var col = new DataGridViewComboBoxColumn
            {
                Name = "Device", HeaderText = "Device",
                FlatStyle = FlatStyle.Flat,
                AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill
            };
            foreach (var dev in _audioDevices)
                col.Items.Add(dev.FriendlyName);
            return col;
        }

        // Resolution and Hz items are populated per-row in EditingControlShowing
        private static DataGridViewComboBoxColumn DynCombo(string name, string header, int width)
        {
            return new DataGridViewComboBoxColumn
            {
                Name = name, HeaderText = header, Width = width, FlatStyle = FlatStyle.Flat
            };
        }

        // ── Events ────────────────────────────────────────────────────────────

        private void DgvDisplays_EditingControlShowing(object sender, DataGridViewEditingControlShowingEventArgs e)
        {
            if (!(e.Control is ComboBox cb)) return;
            int col = _dgvDisplays.CurrentCell?.ColumnIndex ?? -1;
            int row = _dgvDisplays.CurrentCell?.RowIndex   ?? -1;
            if (row < 0) return;

            if (col == 3) // Resolution — populate from modes of the selected monitor
            {
                string gdi  = ExtractGdi(CellStr(_dgvDisplays.Rows[row], 0));
                var    seen = new SortedSet<string>(ResolutionComparer.Instance);
                foreach (var m in GetModes(gdi))
                    if (m.Width > 0 && m.Height > 0) seen.Add(FmtRes(m.Width, m.Height));
                cb.DropDownStyle = ComboBoxStyle.DropDown;
                cb.Items.Clear();
                foreach (var r in seen) cb.Items.Add(r);
            }
            else if (col == 4) // Hz — populate from modes matching GDI + resolution
            {
                string gdi = ExtractGdi(CellStr(_dgvDisplays.Rows[row], 0));
                string res = CellStr(_dgvDisplays.Rows[row], 3);
                var seen = new SortedSet<int>(Comparer<int>.Create((a, b) => b.CompareTo(a)));
                if (TryParseRes(res, out int rw, out int rh))
                    foreach (var m in GetModes(gdi))
                        if (m.Width == rw && m.Height == rh && m.Hz > 0) seen.Add(m.Hz);
                cb.DropDownStyle = ComboBoxStyle.DropDown;
                cb.Items.Clear();
                foreach (var hz in seen) cb.Items.Add(hz.ToString());
            }
        }

        // Auto-fill Type when an audio device is selected
        private void DgvAudio_DeviceChanged(object sender, DataGridViewCellEventArgs e)
        {
            if (e.ColumnIndex != 0 || e.RowIndex < 0) return;
            string name = CellStr(_dgvAudio.Rows[e.RowIndex], 0);
            var dev = _audioDevices.FirstOrDefault(d => d.FriendlyName == name);
            if (dev != null) _dgvAudio.Rows[e.RowIndex].Cells[1].Value = dev.Type;
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
        }

        // ── Populate ──────────────────────────────────────────────────────────

        private void PopulateDisplays(List<ProfileDisplay> items)
        {
            foreach (var d in items)
            {
                string res = d.Width != null && d.Height != null ? FmtRes(d.Width.Value, d.Height.Value) : string.Empty;
                _dgvDisplays.Rows.Add(
                    FormatMonitorByGdi(d.GdiName),
                    BoolStr(d.Active),
                    BoolStr(d.Primary),
                    res,
                    IntStr(d.Hz),
                    IntStr(d.DpiPercent),
                    BoolStr(d.Hdr));
            }
        }

        private void PopulateAudio(List<ProfileAudio> items)
        {
            foreach (var a in items)
                _dgvAudio.Rows.Add(
                    a.Pattern ?? string.Empty,
                    a.Type    ?? "Playback",
                    BoolStr(a.SetDefault),
                    IntStr(a.Volume),
                    BoolStr(a.Mute));
        }

        private void PopulateProcesses(List<StartProcess> items)
        {
            foreach (var p in items)
                _dgvProcesses.Rows.Add(p.Path ?? string.Empty, p.Args ?? string.Empty, BoolStr(p.AsAdmin));
        }

        // ── Save / Delete ─────────────────────────────────────────────────────

        private void BtnSave_Click(object sender, EventArgs e)
        {
            string name = _txtName.Text.Trim();
            if (string.IsNullOrEmpty(name))
            {
                MessageBox.Show("Profile name cannot be empty.", "Validation", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            EditedProfile = new OrchestratorProfile
            {
                Displays       = ReadDisplays(),
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

        // ── Read ──────────────────────────────────────────────────────────────

        private List<ProfileDisplay> ReadDisplays()
        {
            var list = new List<ProfileDisplay>();
            foreach (DataGridViewRow row in _dgvDisplays.Rows)
            {
                string gdi = ExtractGdi(CellStr(row, 0));
                if (string.IsNullOrWhiteSpace(gdi)) continue;
                int? rw = null, rh = null;
                if (TryParseRes(CellStr(row, 3), out int pw, out int ph)) { rw = pw; rh = ph; }
                list.Add(new ProfileDisplay
                {
                    GdiName    = gdi,
                    Active     = ParseBool(CellStr(row, 1)),
                    Primary    = ParseBool(CellStr(row, 2)),
                    Width      = rw,
                    Height     = rh,
                    Hz         = ParseInt(CellStr(row, 4)),
                    DpiPercent = ParseInt(CellStr(row, 5)),
                    Hdr        = ParseBool(CellStr(row, 6))
                });
            }
            return list;
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

        // ── Mode cache ────────────────────────────────────────────────────────

        private List<DisplayModeInfo> GetModes(string gdi)
        {
            if (string.IsNullOrWhiteSpace(gdi)) return new List<DisplayModeInfo>();
            if (!_modeCache.TryGetValue(gdi, out var list))
            {
                try   { list = DisplayManagerAdapter.GetDisplayModes(gdi); }
                catch { list = new List<DisplayModeInfo>(); }
                _modeCache[gdi] = list;
            }
            return list;
        }

        // ── Display name helpers ──────────────────────────────────────────────

        // "DISPLAY1 — Dell U2720Q" format
        private static string FormatMonitor(DisplayDeviceInfo dev)
        {
            string name = dev.MonitorName;
            if (string.IsNullOrWhiteSpace(name)) name = dev.DeviceString;
            return string.IsNullOrWhiteSpace(name) ? dev.GdiName : dev.GdiName + " — " + name;
        }

        private string FormatMonitorByGdi(string gdiName)
        {
            var dev = _displayDevices.FirstOrDefault(d =>
                string.Equals(d.GdiName, gdiName, StringComparison.OrdinalIgnoreCase));
            return dev != null ? FormatMonitor(dev) : (gdiName ?? string.Empty);
        }

        private static string ExtractGdi(string s)
        {
            if (string.IsNullOrEmpty(s)) return string.Empty;
            int sep = s.IndexOf(" — ");
            return sep > 0 ? s.Substring(0, sep).Trim() : s.Trim();
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

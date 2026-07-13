using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using DisplayAudioOrchestrator.CCD;
using SetResolutionAdapters;

// ============================================================
// ResolutionPickerForm — per-display resolution and refresh rate picker.
// Port of Show-ResolutionPickerDialog from PS1.
// ============================================================

namespace DisplayAudioOrchestrator.GUI
{
    public sealed class ResolutionResult
    {
        public int Width  { get; set; }
        public int Height { get; set; }
        public int Hz     { get; set; }
    }

    public sealed class ResolutionPickerForm : Form
    {
        private ComboBox  _comboRes;
        private ComboBox  _comboRate;
        private Label     _lblStatus;
        private Button    _btnApply;
        private Button    _btnSkip;

        private List<DisplayModeInfo>            _modes   = new List<DisplayModeInfo>();
        private Dictionary<string, DisplayModeInfo> _presets = new Dictionary<string, DisplayModeInfo>();

        public ResolutionResult Result { get; private set; }

        private readonly string _nickname;
        private readonly string _gdiName;
        private readonly int    _currentW, _currentH, _currentHz;
        private readonly bool   _applyNow;

        private static readonly int[] CommonWidths = { 3840, 2560, 1920, 1680, 1600, 1440, 1366, 1280, 1024, 800 };
        private static readonly int[] CommonHz     = { 240, 165, 144, 120, 100, 75, 60, 59, 50, 30 };

        public ResolutionPickerForm(string nickname, string gdiName,
            int currentW = 1920, int currentH = 1080, int currentHz = 60, bool applyNow = false)
        {
            _nickname  = nickname;
            _gdiName   = gdiName;
            _currentW  = currentW;
            _currentH  = currentH;
            _currentHz = currentHz;
            _applyNow  = applyNow;

            InitComponents();
        }

        private void InitComponents()
        {
            Text            = $"Resolution – {_nickname}";
            ClientSize      = new Size(400, 200);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition   = FormStartPosition.CenterScreen;
            MaximizeBox     = false;

            var lblRes = new Label { Text = "Resolution:", Left = 10, Top = 12, Width = 100, Height = 22, TextAlign = ContentAlignment.MiddleLeft };
            _comboRes  = new ComboBox { Left = 120, Top = 10, Width = 260, DropDownStyle = ComboBoxStyle.DropDownList };

            var lblRate = new Label { Text = "Refresh Rate:", Left = 10, Top = 50, Width = 100, Height = 22, TextAlign = ContentAlignment.MiddleLeft };
            _comboRate  = new ComboBox { Left = 120, Top = 48, Width = 260, DropDownStyle = ComboBoxStyle.DropDownList };

            _lblStatus = new Label
            {
                Text = "Loading display modes...",
                Left = 10, Top = 82, Width = 374, Height = 22,
                ForeColor = Color.DimGray
            };

            _btnApply = new Button { Text = _applyNow ? "Apply Now" : "Use These Settings", Left = 10, Top = 155, Width = 185, Height = 35 };
            _btnSkip  = new Button { Text = "Skip", Left = 205, Top = 155, Width = 185, Height = 35 };

            Controls.AddRange(new Control[] { lblRes, _comboRes, lblRate, _comboRate, _lblStatus, _btnApply, _btnSkip });

            _comboRes.SelectedIndexChanged += (s, e) => SyncRateCombo();
            _btnApply.Click += BtnApply_Click;
            _btnSkip.Click  += (s, e) => { DialogResult = DialogResult.Cancel; Close(); };

            Shown += Form_Shown;
        }

        private void Form_Shown(object sender, EventArgs e)
        {
            Activate();
            _lblStatus.Text = "Querying supported modes...";
            Application.DoEvents();

            if (!string.IsNullOrEmpty(_gdiName))
            {
                try { _modes = DisplayManagerAdapter.GetDisplayModes(_gdiName); }
                catch { _modes = new List<DisplayModeInfo>(); }
            }

            _lblStatus.Text = _modes.Count > 0
                ? $"{_modes.Count} modes available from Windows."
                : "Could not query display modes — showing common presets.";

            SyncResCombo();
        }

        private void SyncResCombo()
        {
            _comboRes.Items.Clear();
            _presets.Clear();

            var seen = new HashSet<string>();
            if (_modes.Count > 0)
            {
                foreach (var m in _modes)
                {
                    string key = $"{m.Width} x {m.Height}";
                    if (seen.Add(key))
                    {
                        _presets[key] = m;
                        _comboRes.Items.Add(key);
                    }
                }
            }
            else
            {
                foreach (int w in CommonWidths)
                {
                    int h = w == 3840 ? 2160 : w == 2560 ? 1440 : w == 1920 ? 1080
                            : w == 1366 ? 768 : w == 1280 ? 720 : (int)(w * 9.0 / 16);
                    string key = $"{w} x {h}";
                    if (seen.Add(key))
                    {
                        _presets[key] = new DisplayModeInfo { Width = w, Height = h, Hz = 60 };
                        _comboRes.Items.Add(key);
                    }
                }
            }

            // Pre-select current
            string curKey = $"{_currentW} x {_currentH}";
            int selIdx = _comboRes.Items.IndexOf(curKey);
            _comboRes.SelectedIndex = selIdx >= 0 ? selIdx : 0;
            SyncRateCombo();
        }

        private void SyncRateCombo()
        {
            _comboRate.Items.Clear();
            if (_comboRes.SelectedItem == null) return;

            string resKey = _comboRes.SelectedItem.ToString();
            DisplayModeInfo info;
            if (!_presets.TryGetValue(resKey, out info)) return;

            var rates = new List<int>();
            var seenHz = new HashSet<int>();
            if (_modes.Count > 0)
            {
                foreach (var m in _modes)
                    if (m.Width == info.Width && m.Height == info.Height && seenHz.Add(m.Hz))
                        rates.Add(m.Hz);
                rates.Sort((a, b) => b.CompareTo(a));
            }
            if (rates.Count == 0)
                foreach (int hz in CommonHz) rates.Add(hz);

            int bestIdx = 0;
            int bestDelta = int.MaxValue;
            for (int i = 0; i < rates.Count; i++)
            {
                _comboRate.Items.Add($"{rates[i]} Hz");
                int d = Math.Abs(rates[i] - _currentHz);
                if (d < bestDelta) { bestDelta = d; bestIdx = i; }
            }
            _comboRate.SelectedIndex = bestIdx;
        }

        private void BtnApply_Click(object sender, EventArgs e)
        {
            if (_comboRes.SelectedItem == null || _comboRate.SelectedItem == null)
            {
                MessageBox.Show("Pick a resolution and frame rate first.");
                return;
            }

            string resKey = _comboRes.SelectedItem.ToString();
            DisplayModeInfo info;
            if (!_presets.TryGetValue(resKey, out info)) return;

            int hz = int.Parse(_comboRate.SelectedItem.ToString().Replace(" Hz", string.Empty).Trim());

            if (_applyNow && !string.IsNullOrEmpty(_gdiName))
            {
                var result = DisplayManagerAdapter.SetDisplayMode(_gdiName, info.Width, info.Height, hz);
                OrchestratorLogger.Log($"ResolutionPicker: SetDisplayMode {_gdiName} {info.Width}x{info.Height}@{hz}Hz → {result}", LogLevel.Info);
            }

            Result = new ResolutionResult { Width = info.Width, Height = info.Height, Hz = hz };
            DialogResult = DialogResult.OK;
            Close();
        }
    }
}

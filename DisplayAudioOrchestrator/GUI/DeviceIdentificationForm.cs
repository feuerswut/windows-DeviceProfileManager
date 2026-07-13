using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;
using DisplayAudioOrchestrator.Orchestrator;

// ============================================================
// DeviceIdentificationForm — tabbed wizard to register display and audio nicknames.
// Port of Show-DeviceIdentificationGui from PS1.
// ============================================================

namespace DisplayAudioOrchestrator.GUI
{
    public sealed class DeviceIdentificationForm : Form
    {
        private DeviceState _state;
        private TabControl  _tabs;
        private ListView    _lvDisplays;
        private ListView    _lvAudio;

        public DeviceIdentificationForm(DeviceState state)
        {
            _state = state;
            InitComponents();
            LoadData();
        }

        private void InitComponents()
        {
            Text            = "Device Identification Wizard";
            ClientSize      = new Size(680, 420);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition   = FormStartPosition.CenterScreen;
            MaximizeBox     = false;

            _tabs = new TabControl { Left = 8, Top = 8, Width = 664, Height = 320 };
            Controls.Add(_tabs);

            // ── Displays tab ──────────────────────────────────────────────────
            var tabDisplays = new TabPage("Displays");
            _tabs.TabPages.Add(tabDisplays);

            _lvDisplays = new ListView
            {
                View          = View.Details,
                FullRowSelect = true,
                GridLines     = true,
                Dock          = DockStyle.Fill
            };
            _lvDisplays.Columns.Add("GDI Name",     80);
            _lvDisplays.Columns.Add("Friendly Name", 180);
            _lvDisplays.Columns.Add("Active",        50);
            _lvDisplays.Columns.Add("Resolution",   100);
            _lvDisplays.Columns.Add("Nickname",      120);
            tabDisplays.Controls.Add(_lvDisplays);

            var pnlDisplayButtons = new FlowLayoutPanel
            {
                Dock = DockStyle.None, Left = 8, Top = 330, Width = 664, Height = 40,
                FlowDirection = FlowDirection.LeftToRight
            };
            Controls.Add(pnlDisplayButtons);

            var btnIdentify = new Button { Text = "Flash Overlays", Width = 130, Height = 35 };
            btnIdentify.Click += (s, e) =>
            {
                MonitorOverlayForm.ShowOverlays();
                LoadData();
            };
            pnlDisplayButtons.Controls.Add(btnIdentify);

            var btnRegisterDisplay = new Button { Text = "Register Nickname", Width = 140, Height = 35 };
            btnRegisterDisplay.Click += BtnRegisterDisplay_Click;
            pnlDisplayButtons.Controls.Add(btnRegisterDisplay);

            var btnRemoveDisplay = new Button { Text = "Remove Nickname", Width = 130, Height = 35 };
            btnRemoveDisplay.Click += BtnRemoveDisplay_Click;
            pnlDisplayButtons.Controls.Add(btnRemoveDisplay);

            var btnRefresh = new Button { Text = "Refresh", Width = 80, Height = 35 };
            btnRefresh.Click += (s, e) => LoadData();
            pnlDisplayButtons.Controls.Add(btnRefresh);

            // ── Audio tab ─────────────────────────────────────────────────────
            var tabAudio = new TabPage("Audio");
            _tabs.TabPages.Add(tabAudio);

            _lvAudio = new ListView
            {
                View          = View.Details,
                FullRowSelect = true,
                GridLines     = true,
                Dock          = DockStyle.Fill
            };
            _lvAudio.Columns.Add("Type",          70);
            _lvAudio.Columns.Add("Friendly Name", 230);
            _lvAudio.Columns.Add("Default",       55);
            _lvAudio.Columns.Add("Volume",        60);
            _lvAudio.Columns.Add("Nickname",      130);
            tabAudio.Controls.Add(_lvAudio);

            var pnlAudioButtons = new FlowLayoutPanel
            {
                Dock = DockStyle.None, Left = 8, Top = 330, Width = 664, Height = 40,
                FlowDirection = FlowDirection.LeftToRight
            };
            Controls.Add(pnlAudioButtons);

            var btnRegisterAudio = new Button { Text = "Register Nickname", Width = 140, Height = 35 };
            btnRegisterAudio.Click += BtnRegisterAudio_Click;
            pnlAudioButtons.Controls.Add(btnRegisterAudio);

            var btnRemoveAudio = new Button { Text = "Remove Nickname", Width = 130, Height = 35 };
            btnRemoveAudio.Click += BtnRemoveAudio_Click;
            pnlAudioButtons.Controls.Add(btnRemoveAudio);

            var btnRefreshAudio = new Button { Text = "Refresh", Width = 80, Height = 35 };
            btnRefreshAudio.Click += (s, e) => LoadData();
            pnlAudioButtons.Controls.Add(btnRefreshAudio);

            // ── Bottom ────────────────────────────────────────────────────────
            var btnClose = new Button { Text = "Close", Left = 590, Top = 382, Width = 80, Height = 30 };
            btnClose.Click += (s, e) => Close();
            Controls.Add(btnClose);
        }

        private void LoadData()
        {
            LoadDisplays();
            LoadAudio();
        }

        private void LoadDisplays()
        {
            _lvDisplays.Items.Clear();
            List<DisplayInfo> displays;
            try { displays = DisplayConfigManager.GetAllDisplayInfo(); }
            catch { displays = new List<DisplayInfo>(); }

            foreach (var d in displays)
            {
                string nick = FindDisplayNickname(d);
                string res  = d.Active ? $"{d.Width}x{d.Height}@{d.Hz}Hz" : "-";
                var item = new ListViewItem(d.GdiShortName);
                item.SubItems.Add(d.FriendlyName ?? string.Empty);
                item.SubItems.Add(d.Active ? "Yes" : "No");
                item.SubItems.Add(res);
                item.SubItems.Add(nick ?? string.Empty);
                item.Tag = d;
                if (!d.Active) item.ForeColor = Color.DimGray;
                _lvDisplays.Items.Add(item);
            }
        }

        private void LoadAudio()
        {
            _lvAudio.Items.Clear();
            List<AudioDeviceInfo> devices;
            try { devices = AudioManager.GetAllDevices(); }
            catch { devices = new List<AudioDeviceInfo>(); }

            foreach (var d in devices)
            {
                if (d.State != AudioGuids.DEVICE_STATE_ACTIVE) continue;
                string nick = FindAudioNickname(d);
                var item = new ListViewItem(d.Type);
                item.SubItems.Add(d.FriendlyName ?? string.Empty);
                item.SubItems.Add(d.IsDefault ? "Yes" : string.Empty);
                item.SubItems.Add(d.VolumePercent >= 0 ? d.VolumePercent + "%" : string.Empty);
                item.SubItems.Add(nick ?? string.Empty);
                item.Tag = d;
                _lvAudio.Items.Add(item);
            }
        }

        private string FindDisplayNickname(DisplayInfo d)
        {
            foreach (var kv in _state.Displays)
            {
                var reg = kv.Value;
                if (d.GdiShortName.Equals(reg.GdiName, StringComparison.OrdinalIgnoreCase)) return kv.Key;
                if (!string.IsNullOrEmpty(reg.FriendlyName) && d.FriendlyName != null &&
                    d.FriendlyName.IndexOf(reg.FriendlyName, StringComparison.OrdinalIgnoreCase) >= 0)
                    return kv.Key;
            }
            return null;
        }

        private string FindAudioNickname(AudioDeviceInfo d)
        {
            foreach (var kv in _state.Audio)
            {
                var reg = kv.Value;
                if (!string.IsNullOrEmpty(reg.Pattern) &&
                    d.FriendlyName.IndexOf(reg.Pattern, StringComparison.OrdinalIgnoreCase) >= 0)
                    return kv.Key;
            }
            return null;
        }

        // ── Register / remove display nicknames ───────────────────────────────

        private void BtnRegisterDisplay_Click(object sender, EventArgs e)
        {
            if (_lvDisplays.SelectedItems.Count == 0)
            {
                MessageBox.Show("Select a display first.", "Register", MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }
            var d = (DisplayInfo)_lvDisplays.SelectedItems[0].Tag;
            string suggested = FindDisplayNickname(d) ?? SuggestNickname(d.FriendlyName, "DISPLAY");
            string nick = PromptText($"Nickname for {d.GdiShortName} ({d.FriendlyName}):", suggested);
            if (string.IsNullOrWhiteSpace(nick)) return;

            NicknameRegistry.RegisterDisplay(_state, nick.Trim(), d.FriendlyName, d.GdiShortName);
            StateStore.Save(_state);
            LoadDisplays();
        }

        private void BtnRemoveDisplay_Click(object sender, EventArgs e)
        {
            if (_lvDisplays.SelectedItems.Count == 0) return;
            var d    = (DisplayInfo)_lvDisplays.SelectedItems[0].Tag;
            string nick = FindDisplayNickname(d);
            if (nick == null) { MessageBox.Show("No nickname assigned to this display."); return; }
            if (MessageBox.Show($"Remove nickname '{nick}'?", "Remove", MessageBoxButtons.YesNo) != DialogResult.Yes) return;
            NicknameRegistry.RemoveDisplay(_state, nick);
            StateStore.Save(_state);
            LoadDisplays();
        }

        // ── Register / remove audio nicknames ─────────────────────────────────

        private void BtnRegisterAudio_Click(object sender, EventArgs e)
        {
            if (_lvAudio.SelectedItems.Count == 0)
            {
                MessageBox.Show("Select an audio device first.", "Register", MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }
            var d = (AudioDeviceInfo)_lvAudio.SelectedItems[0].Tag;
            string suggested = FindAudioNickname(d) ?? SuggestNickname(d.FriendlyName, "AUDIO");
            string nick = PromptText($"Nickname for {d.FriendlyName} ({d.Type}):", suggested);
            if (string.IsNullOrWhiteSpace(nick)) return;

            string pattern = PromptText("Match pattern (substring of device name):", d.FriendlyName);
            if (string.IsNullOrWhiteSpace(pattern)) return;

            NicknameRegistry.RegisterAudio(_state, nick.Trim(), pattern.Trim(), d.Type, d.Id);
            StateStore.Save(_state);
            LoadAudio();
        }

        private void BtnRemoveAudio_Click(object sender, EventArgs e)
        {
            if (_lvAudio.SelectedItems.Count == 0) return;
            var d    = (AudioDeviceInfo)_lvAudio.SelectedItems[0].Tag;
            string nick = FindAudioNickname(d);
            if (nick == null) { MessageBox.Show("No nickname assigned to this device."); return; }
            if (MessageBox.Show($"Remove nickname '{nick}'?", "Remove", MessageBoxButtons.YesNo) != DialogResult.Yes) return;
            NicknameRegistry.RemoveAudio(_state, nick);
            StateStore.Save(_state);
            LoadAudio();
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        private static string SuggestNickname(string friendly, string prefix)
        {
            if (string.IsNullOrEmpty(friendly)) return prefix;
            string[] words = friendly.Split(new[] { ' ', '-', '_' }, StringSplitOptions.RemoveEmptyEntries);
            if (words.Length >= 2) return (words[0] + "_" + words[1]).ToUpperInvariant();
            return friendly.ToUpperInvariant().Replace(" ", "_");
        }

        private static string PromptText(string prompt, string defaultValue = "")
        {
            using (var dlg = new Form())
            {
                dlg.Text = "Input";
                dlg.ClientSize = new Size(420, 120);
                dlg.FormBorderStyle = FormBorderStyle.FixedDialog;
                dlg.StartPosition = FormStartPosition.CenterParent;
                dlg.MaximizeBox = false;

                var lbl = new Label { Text = prompt, Left = 10, Top = 10, Width = 400, Height = 32, TextAlign = ContentAlignment.MiddleLeft };
                var txt = new TextBox { Left = 10, Top = 46, Width = 400, Text = defaultValue ?? string.Empty };
                var btnOk     = new Button { Text = "OK",     Left = 230, Top = 80, Width = 80, Height = 28, DialogResult = DialogResult.OK };
                var btnCancel = new Button { Text = "Cancel", Left = 320, Top = 80, Width = 90, Height = 28, DialogResult = DialogResult.Cancel };

                dlg.Controls.AddRange(new Control[] { lbl, txt, btnOk, btnCancel });
                dlg.AcceptButton = btnOk;
                dlg.CancelButton = btnCancel;

                return dlg.ShowDialog() == DialogResult.OK ? txt.Text : null;
            }
        }
    }
}

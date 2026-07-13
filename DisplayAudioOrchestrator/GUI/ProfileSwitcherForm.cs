using System;
using System.Collections.Generic;
using System.Drawing;
using System.Threading;
using System.Windows.Forms;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;
using DisplayAudioOrchestrator.Orchestrator;

// ============================================================
// ProfileSwitcherForm — main GUI window.
// Subscribes to OrchestratorLogger.LogEvent for WARN+ERROR output box.
// All log levels always go to Console (handled by the logger itself).
// ============================================================

namespace DisplayAudioOrchestrator.GUI
{
    public sealed class ProfileSwitcherForm : Form
    {
        private ListBox  _lstProfiles;
        private TextBox  _txtOutput;
        private Button   _btnApply;
        private Button   _btnSave;
        private Button   _btnDelete;
        private Button   _btnFlash;
        private Button   _btnBrowseDevices;
        private Label    _lblStatus;
        private DeviceState _state;

        public ProfileSwitcherForm()
        {
            _state = StateStore.Load();
            InitComponents();
            OrchestratorLogger.LogEvent += OnLogEvent;
            Disposed += (s, e) => OrchestratorLogger.LogEvent -= OnLogEvent;
            RefreshProfiles();
            RefreshStatus();
        }

        private void InitComponents()
        {
            Text            = "Display Audio Orchestrator";
            ClientSize      = new Size(720, 540);
            FormBorderStyle = FormBorderStyle.Sizable;
            StartPosition   = FormStartPosition.CenterScreen;
            MinimumSize     = new Size(560, 440);

            const int actionH = 60;
            const int statusH = 24;
            const int pad     = 8;
            const int profW   = 220;

            // ── Left: profile list ────────────────────────────────────────────
            var grpProfiles = new GroupBox
            {
                Text   = "Profiles",
                Left   = pad,
                Top    = pad,
                Width  = profW,
                Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Bottom
            };
            Controls.Add(grpProfiles);

            _lstProfiles = new ListBox
            {
                Left   = 8,
                Top    = 18,
                Width  = profW - 18,
                Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom
            };
            _lstProfiles.DoubleClick += (s, e) => ApplySelectedProfile();
            grpProfiles.Controls.Add(_lstProfiles);

            _btnApply  = new Button { Text = "Apply",  Left = 8,   Width = 95, Height = 28, Anchor = AnchorStyles.Bottom | AnchorStyles.Left };
            _btnDelete = new Button { Text = "Delete", Left = 109, Width = 95, Height = 28, Anchor = AnchorStyles.Bottom | AnchorStyles.Left };
            grpProfiles.Controls.AddRange(new Control[] { _btnApply, _btnDelete });

            _btnApply.Click  += (s, e) => ApplySelectedProfile();
            _btnDelete.Click += BtnDelete_Click;

            // ── Right: output box ─────────────────────────────────────────────
            var grpOutput = new GroupBox
            {
                Text   = "Output (Warnings and Errors)",
                Left   = profW + pad * 2,
                Top    = pad,
                Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom
            };
            Controls.Add(grpOutput);

            _txtOutput = new TextBox
            {
                Multiline  = true,
                ScrollBars = ScrollBars.Vertical,
                ReadOnly   = true,
                Left       = 8,
                Top        = 18,
                Anchor     = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Bottom,
                Font       = new Font("Segoe UI", 9f),
                BackColor  = Color.White,
                ForeColor  = Color.Black,
                WordWrap   = true
            };
            grpOutput.Controls.Add(_txtOutput);

            // ── Bottom: actions bar ───────────────────────────────────────────
            var pnlActions = new Panel
            {
                Left   = pad,
                Height = actionH,
                Anchor = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right
            };
            Controls.Add(pnlActions);

            _btnSave = new Button { Text = "Save Current State as Profile...", Left = 0,   Top = 8, Width = 230, Height = 30 };
            _btnSave.Click += BtnSave_Click;

            _btnFlash = new Button { Text = "Flash Monitor Overlays", Left = 238, Top = 8, Width = 170, Height = 30 };
            _btnFlash.Click += (s, e) => MonitorOverlayForm.ShowOverlays();

            _btnBrowseDevices = new Button { Text = "Browse Devices...", Left = 416, Top = 8, Width = 140, Height = 30 };
            _btnBrowseDevices.Click += BtnBrowseDevices_Click;

            pnlActions.Controls.AddRange(new Control[] { _btnSave, _btnFlash, _btnBrowseDevices });

            // ── Status label ──────────────────────────────────────────────────
            _lblStatus = new Label
            {
                Left      = pad,
                Height    = statusH,
                Anchor    = AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right,
                ForeColor = Color.DimGray
            };
            Controls.Add(_lblStatus);

            Resize += (s, e) => DoLayout();
            Load   += (s, e) => DoLayout();
        }

        private void DoLayout()
        {
            const int pad     = 8;
            const int profW   = 220;
            const int actionH = 60;
            const int statusH = 24;

            int w = ClientSize.Width;
            int h = ClientSize.Height;

            int topH = h - actionH - statusH - pad * 3;

            var grpProfiles = Controls[0] as GroupBox;
            if (grpProfiles != null)
            {
                grpProfiles.SetBounds(pad, pad, profW, topH);
                if (grpProfiles.Controls.Count > 0)
                    grpProfiles.Controls[0].SetBounds(8, 18, profW - 18, topH - 62);
                if (grpProfiles.Controls.Count >= 3)
                {
                    grpProfiles.Controls[1].Top = topH - 36;
                    grpProfiles.Controls[2].Top = topH - 36;
                }
            }

            var grpOutput = Controls.Count > 1 ? Controls[1] as GroupBox : null;
            if (grpOutput != null)
            {
                int ox = profW + pad * 2;
                grpOutput.SetBounds(ox, pad, w - ox - pad, topH);
                if (grpOutput.Controls.Count > 0)
                    grpOutput.Controls[0].SetBounds(8, 18, grpOutput.Width - 18, topH - 36);
            }

            var pnlActions = Controls.Count > 2 ? Controls[2] as Panel : null;
            if (pnlActions != null)
                pnlActions.SetBounds(pad, pad + topH + pad, w - pad * 2, actionH);

            var lbl = Controls.Count > 3 ? Controls[3] as Label : null;
            if (lbl != null)
                lbl.SetBounds(pad, h - statusH - pad, w - pad * 2, statusH);
        }

        // ── Logger subscription ───────────────────────────────────────────────

        private void OnLogEvent(string message, LogLevel level)
        {
            if (level < LogLevel.Warn) return;

            Action append = () =>
            {
                if (IsDisposed || !IsHandleCreated) return;
                _txtOutput.AppendText(message + Environment.NewLine);
                _txtOutput.SelectionStart = _txtOutput.Text.Length;
                _txtOutput.ScrollToCaret();
                _lblStatus.Text = message.Length > 100 ? message.Substring(0, 100) + "..." : message;
            };

            if (InvokeRequired) BeginInvoke(append);
            else append();
        }

        // ── Profile list ──────────────────────────────────────────────────────

        private void RefreshProfiles()
        {
            _state = StateStore.Load();
            _lstProfiles.Items.Clear();
            foreach (var kv in _state.Profiles)
                _lstProfiles.Items.Add(kv.Key);
            _btnApply.Enabled  = _lstProfiles.Items.Count > 0;
            _btnDelete.Enabled = _lstProfiles.Items.Count > 0;
        }

        private void RefreshStatus()
        {
            var displays = new List<DisplayInfo>();
            try { displays = DisplayConfigManager.GetAllDisplayInfo(); } catch { }
            int active = 0;
            foreach (var d in displays) if (d.Active) active++;
            _lblStatus.Text = $"{active} display(s) active. {_state.Profiles.Count} profile(s) saved. " +
                              "Double-click a profile to apply.";
        }

        // ── Apply ─────────────────────────────────────────────────────────────

        private void ApplySelectedProfile()
        {
            if (_lstProfiles.SelectedItem == null) return;
            string name = _lstProfiles.SelectedItem.ToString();
            OrchestratorLogger.Log($"GUI: applying profile '{name}'", LogLevel.Info);

            _btnApply.Enabled = false;
            var thread = new Thread(() =>
            {
                try
                {
                    ProfileManager.Apply(name, _state);
                    OrchestratorLogger.Log($"Profile '{name}' applied.", LogLevel.Info);
                }
                catch (ProfileNotAppliedException ex)
                {
                    OrchestratorLogger.Log(ex.Message, LogLevel.Error);
                }
                catch (Exception ex)
                {
                    OrchestratorLogger.Log($"Unexpected error: {ex.Message}", LogLevel.Error);
                }
                finally
                {
                    if (!IsDisposed && IsHandleCreated)
                        BeginInvoke(new Action(() =>
                        {
                            _btnApply.Enabled = true;
                            RefreshStatus();
                        }));
                }
            });
            thread.SetApartmentState(ApartmentState.STA);
            thread.Start();
        }

        // ── Save profile ──────────────────────────────────────────────────────

        private void BtnSave_Click(object sender, EventArgs e)
        {
            string name;
            using (var dlg = new Form())
            {
                dlg.Text            = "Save Profile";
                dlg.ClientSize      = new Size(380, 110);
                dlg.FormBorderStyle = FormBorderStyle.FixedDialog;
                dlg.StartPosition   = FormStartPosition.CenterParent;
                dlg.MaximizeBox     = false;

                var lbl = new Label  { Text = "Profile name:", Left = 10, Top = 14, Width = 100, Height = 22 };
                var txt = new TextBox { Left = 120, Top = 12, Width = 240 };
                var ok  = new Button { Text = "Save",   Left = 200, Top = 70, Width = 80, Height = 28, DialogResult = DialogResult.OK };
                var can = new Button { Text = "Cancel", Left = 288, Top = 70, Width = 80, Height = 28, DialogResult = DialogResult.Cancel };
                dlg.Controls.AddRange(new Control[] { lbl, txt, ok, can });
                dlg.AcceptButton = ok;
                dlg.CancelButton = can;

                if (dlg.ShowDialog() != DialogResult.OK || string.IsNullOrWhiteSpace(txt.Text)) return;
                name = txt.Text.Trim();
            }

            SaveCurrentState(name);
        }

        private void SaveCurrentState(string profileName)
        {
            OrchestratorLogger.Log($"Saving current state as profile '{profileName}'...", LogLevel.Info);
            try
            {
                var displays = DisplayConfigManager.GetAllDisplayInfo();
                var audio    = AudioManager.GetAllDevices();

                var profile = new OrchestratorProfile();

                foreach (var d in displays)
                {
                    if (!d.Active) continue;
                    profile.Displays.Add(new ProfileDisplay
                    {
                        GdiName    = d.GdiShortName,
                        Active     = true,
                        Primary    = d.Primary,
                        Width      = d.Width,
                        Height     = d.Height,
                        Hz         = d.Hz,
                        DpiPercent = d.DpiPercent,
                        Hdr        = d.HdrEnabled
                    });
                }

                foreach (var a in audio)
                {
                    if (a.State != AudioGuids.DEVICE_STATE_ACTIVE) continue;
                    profile.Audio.Add(new ProfileAudio
                    {
                        Pattern    = a.FriendlyName,
                        Type       = a.Type,
                        SetDefault = a.IsDefault,
                        Volume     = a.VolumePercent >= 0 ? (int?)a.VolumePercent : null
                    });
                }

                _state.Profiles[profileName] = profile;
                StateStore.Save(_state);
                RefreshProfiles();
                OrchestratorLogger.Log($"Profile '{profileName}' saved.", LogLevel.Info);
                MessageBox.Show($"Profile '{profileName}' saved.", "Saved",
                    MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            catch (Exception ex)
            {
                OrchestratorLogger.Log($"Failed to save profile: {ex.Message}", LogLevel.Error);
            }
        }

        // ── Delete profile ────────────────────────────────────────────────────

        private void BtnDelete_Click(object sender, EventArgs e)
        {
            if (_lstProfiles.SelectedItem == null) return;
            string name = _lstProfiles.SelectedItem.ToString();
            if (MessageBox.Show($"Delete profile '{name}'?", "Confirm", MessageBoxButtons.YesNo,
                    MessageBoxIcon.Question) != DialogResult.Yes) return;
            _state.Profiles.Remove(name);
            StateStore.Save(_state);
            RefreshProfiles();
        }

        // ── Browse devices ────────────────────────────────────────────────────

        private void BtnBrowseDevices_Click(object sender, EventArgs e)
        {
            using (var dlg = new DeviceIdentificationForm())
                dlg.ShowDialog(this);
        }
    }
}

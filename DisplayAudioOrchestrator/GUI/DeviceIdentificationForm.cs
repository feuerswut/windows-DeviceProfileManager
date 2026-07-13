using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using DisplayAudioOrchestrator.Audio;
using DisplayAudioOrchestrator.CCD;

// ============================================================
// DeviceIdentificationForm — display and audio browser.
// No nickname registration; displays are always identified by GDI name (DISPLAY1).
// Use this form to see what's active and flash overlays.
// ============================================================

namespace DisplayAudioOrchestrator.GUI
{
    public sealed class DeviceIdentificationForm : Form
    {
        private ListView _lvDisplays;
        private ListView _lvAudio;

        public DeviceIdentificationForm()
        {
            InitComponents();
            LoadData();
        }

        private void InitComponents()
        {
            Text            = "Device Browser";
            ClientSize      = new Size(680, 400);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition   = FormStartPosition.CenterScreen;
            MaximizeBox     = false;

            var tabs = new TabControl { Left = 8, Top = 8, Width = 664, Height = 384 };
            Controls.Add(tabs);

            // ── Displays tab ──────────────────────────────────────────────────
            var tabDisplays = new TabPage("Displays");
            tabs.TabPages.Add(tabDisplays);

            var pnlDispBtns = new Panel { Dock = DockStyle.Bottom, Height = 42 };
            tabDisplays.Controls.Add(pnlDispBtns);

            _lvDisplays = new ListView
            {
                View = View.Details, FullRowSelect = true, GridLines = true, Dock = DockStyle.Fill
            };
            _lvDisplays.Columns.Add("GDI Name",      90);
            _lvDisplays.Columns.Add("Friendly Name", 200);
            _lvDisplays.Columns.Add("Active",         52);
            _lvDisplays.Columns.Add("Resolution",    130);
            _lvDisplays.Columns.Add("DPI",            45);
            _lvDisplays.Columns.Add("HDR",            40);
            tabDisplays.Controls.Add(_lvDisplays);

            var btnFlash   = new Button { Text = "Flash Overlays", Left = 4,   Top = 6, Width = 120, Height = 30 };
            var btnRefreshD = new Button { Text = "Refresh",       Left = 130,  Top = 6, Width =  80, Height = 30 };
            var btnCloseD  = new Button { Text = "Close",          Left = 568,  Top = 6, Width =  78, Height = 30 };
            btnFlash.Click    += (s, e) => { MonitorOverlayForm.ShowOverlays(); LoadDisplays(); };
            btnRefreshD.Click += (s, e) => LoadDisplays();
            btnCloseD.Click   += (s, e) => Close();
            pnlDispBtns.Controls.AddRange(new Control[] { btnFlash, btnRefreshD, btnCloseD });

            // ── Audio tab ─────────────────────────────────────────────────────
            var tabAudio = new TabPage("Audio");
            tabs.TabPages.Add(tabAudio);

            var pnlAudBtns = new Panel { Dock = DockStyle.Bottom, Height = 42 };
            tabAudio.Controls.Add(pnlAudBtns);

            _lvAudio = new ListView
            {
                View = View.Details, FullRowSelect = true, GridLines = true, Dock = DockStyle.Fill
            };
            _lvAudio.Columns.Add("Type",          70);
            _lvAudio.Columns.Add("Friendly Name", 290);
            _lvAudio.Columns.Add("Default",        55);
            _lvAudio.Columns.Add("Volume",         55);
            _lvAudio.Columns.Add("State",          60);
            tabAudio.Controls.Add(_lvAudio);

            var btnRefreshA = new Button { Text = "Refresh", Left = 4,   Top = 6, Width = 80, Height = 30 };
            var btnCloseA   = new Button { Text = "Close",   Left = 568,  Top = 6, Width = 78, Height = 30 };
            btnRefreshA.Click += (s, e) => LoadAudio();
            btnCloseA.Click   += (s, e) => Close();
            pnlAudBtns.Controls.AddRange(new Control[] { btnRefreshA, btnCloseA });
        }

        private void LoadData() { LoadDisplays(); LoadAudio(); }

        private void LoadDisplays()
        {
            _lvDisplays.Items.Clear();
            List<DisplayInfo> displays;
            try { displays = DisplayConfigManager.GetAllDisplayInfo(); }
            catch { displays = new List<DisplayInfo>(); }

            foreach (var d in displays)
            {
                string res = d.Active ? $"{d.Width}x{d.Height}@{d.Hz}Hz" : "-";
                var item = new ListViewItem(d.GdiShortName);
                item.SubItems.Add(d.FriendlyName ?? string.Empty);
                item.SubItems.Add(d.Active ? "Yes" : "No");
                item.SubItems.Add(res);
                item.SubItems.Add(d.Active ? d.DpiPercent + "%" : string.Empty);
                item.SubItems.Add(d.Active && d.HdrEnabled ? "On" : string.Empty);
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
                var item = new ListViewItem(d.Type);
                item.SubItems.Add(d.FriendlyName ?? string.Empty);
                item.SubItems.Add(d.IsDefault ? "Yes" : string.Empty);
                item.SubItems.Add(d.VolumePercent >= 0 ? d.VolumePercent + "%" : string.Empty);
                item.SubItems.Add(d.State == AudioGuids.DEVICE_STATE_ACTIVE ? "Active" : "Inactive");
                if (d.State != AudioGuids.DEVICE_STATE_ACTIVE) item.ForeColor = Color.DimGray;
                _lvAudio.Items.Add(item);
            }
        }
    }
}

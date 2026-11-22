import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

const busName = 'io.github.gracepetermutiibwa.PookieClip';
const objectPath = '/io/github/gracepetermutiibwa/PookieClip';
const interfaceName = 'io.github.gracepetermutiibwa.PookieClip';

// Password managers tag secret copies with this type so clipboard tools skip them
const secretHint = 'x-kde-passwordManagerHint';
const maxBytes = 20 * 1024 * 1024;

const imageTypes = ['image/png', 'image/jpeg', 'image/webp'];
const textTypes = ['text/plain;charset=utf-8', 'UTF8_STRING', 'text/plain', 'STRING'];

// Top bar icon with no menu, a click opens or hides the panel
const PookieClipIndicator = GObject.registerClass(
class PookieClipIndicator extends PanelMenu.Button {
    constructor(extension) {
        super(0.5, 'PookieClip', true);
        this._extension = extension;
        this.add_child(new St.Icon({
            gicon: Gio.icon_new_for_string(`${extension.path}/icons/pookieclip-symbolic.svg`),
            style_class: 'system-status-icon',
        }));
    }

    vfunc_event(event) {
        const type = event.type();
        if (type === Clutter.EventType.BUTTON_PRESS || type === Clutter.EventType.TOUCH_BEGIN) {
            this._extension._togglePanel();
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }
});

export default class PookieClipExtension extends Extension {
    enable() {
        this._indicator = new PookieClipIndicator(this);
        Main.panel.addToStatusArea(this.uuid, this._indicator);

        // Ctrl+Alt+H by default, changeable through the toggle-panel setting
        this._settings = this.getSettings();
        Main.wm.addKeybinding('toggle-panel', this._settings, Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW, () => this._togglePanel());

        this._selection = global.display.get_selection();
        this._clipboard = St.Clipboard.get_default();
        this._ownerChangedId = this._selection.connect('owner-changed', (_selection, selectionType) => {
            if (selectionType === Meta.SelectionType.SELECTION_CLIPBOARD)
                this._forwardClipboard();
        });
    }

    disable() {
        Main.wm.removeKeybinding('toggle-panel');
        this._settings = null;
        this._indicator.destroy();
        this._indicator = null;
        this._selection.disconnect(this._ownerChangedId);
        this._ownerChangedId = null;
        this._selection = null;
        this._clipboard = null;
    }

    _forwardClipboard() {
        const mimeTypes = this._selection.get_mimetypes(Meta.SelectionType.SELECTION_CLIPBOARD);
        if (mimeTypes.includes(secretHint)) {
            this._send('AddSkipped', null);
            return;
        }

        const mimeType = this._pickMimeType(mimeTypes);
        if (!mimeType)
            return;

        const sourceApp = this._focusedAppName();
        this._clipboard.get_content(St.ClipboardType.CLIPBOARD, mimeType, (_clipboard, bytes) => {
            if (!bytes || bytes.get_size() === 0 || bytes.get_size() > maxBytes)
                return;
            this._send('AddClip', new GLib.Variant('(ssay)', [mimeType, sourceApp, bytes.get_data()]));
        });
    }

    // Images first, then copied files, then plain text
    _pickMimeType(mimeTypes) {
        return imageTypes.find(type => mimeTypes.includes(type))
            ?? (mimeTypes.includes('text/uri-list') ? 'text/uri-list' : null)
            ?? textTypes.find(type => mimeTypes.includes(type))
            ?? null;
    }

    _focusedAppName() {
        const window = global.display.focus_window;
        if (!window)
            return '';
        const app = Shell.WindowTracker.get_default().get_window_app(window);
        return app ? app.get_name() : (window.get_wm_class() ?? '');
    }

    // Fire and forget, so nothing breaks when PookieClip isn't running
    _send(method, parameters) {
        Gio.DBus.session.call(busName, objectPath, interfaceName, method, parameters,
            null, Gio.DBusCallFlags.NO_AUTO_START, -1, null, null);
    }

    // Opens or hides the panel, and starts PookieClip when it isn't running
    _togglePanel() {
        Gio.DBus.session.call(busName, objectPath, interfaceName, 'Toggle', null,
            null, Gio.DBusCallFlags.NO_AUTO_START, -1, null, (connection, result) => {
                try {
                    connection.call_finish(result);
                } catch {
                    this._launchApp();
                }
            });
    }

    _launchApp() {
        const program = GLib.find_program_in_path('pookieclip');
        if (program)
            GLib.spawn_async(null, [program], null, GLib.SpawnFlags.DEFAULT, null);
    }
}
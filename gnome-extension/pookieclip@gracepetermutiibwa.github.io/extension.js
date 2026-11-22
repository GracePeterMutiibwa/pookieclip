import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const busName = 'io.github.gracepetermutiibwa.PookieClip';
const objectPath = '/io/github/gracepetermutiibwa/PookieClip';
const interfaceName = 'io.github.gracepetermutiibwa.PookieClip';

// Password managers tag secret copies with this type so clipboard tools skip them
const secretHint = 'x-kde-passwordManagerHint';
const maxBytes = 20 * 1024 * 1024;

const imageTypes = ['image/png', 'image/jpeg', 'image/webp'];
const textTypes = ['text/plain;charset=utf-8', 'UTF8_STRING', 'text/plain', 'STRING'];

export default class PookieClipExtension extends Extension {
    enable() {
        this._selection = global.display.get_selection();
        this._clipboard = St.Clipboard.get_default();
        this._ownerChangedId = this._selection.connect('owner-changed', (_selection, selectionType) => {
            if (selectionType === Meta.SelectionType.SELECTION_CLIPBOARD)
                this._forwardClipboard();
        });
    }

    disable() {
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
}
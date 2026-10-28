//! Representative Android `uiautomator dump` XML test fixtures.
//!
//! Provides realistic layouts for headless unit & integration testing:
//! 1. `settings_app()`: System settings screen with toggles, search, and list items.
//! 2. `calculator_app()`: Full numeric keypad and display screen.
//! 3. `home_lock_screen()`: Lock screen with clock, emergency, unlock swipe, camera.
//! 4. `login_form_dialog()`: Authentication dialog with edit fields, checkbox, and action buttons.

/// Realistic Android Settings application hierarchy XML fixture.
pub fn settings_app_xml() -> &'static str {
    r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.android.settings" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][1080,2400]">
    <node index="0" text="Search settings" resource-id="com.android.settings:id/search_action_bar" class="android.widget.EditText" package="com.android.settings" content-desc="Search settings" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="true" password="false" selected="false" bounds="[48,120][1032,240]" />
    <node index="1" text="Settings" resource-id="com.android.settings:id/homepage_title" class="android.widget.TextView" package="com.android.settings" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[72,280][500,380]" />
    <node index="2" text="Network &amp; internet" resource-id="com.android.settings:id/network_tile" class="android.widget.TextView" package="com.android.settings" content-desc="Wi-Fi, Mobile, Hotspot" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[72,420][1008,540]" />
    <node index="3" text="Connected devices" resource-id="com.android.settings:id/connected_tile" class="android.widget.TextView" package="com.android.settings" content-desc="Bluetooth, Pairing" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[72,560][1008,680]" />
    <node index="4" text="Apps" resource-id="com.android.settings:id/apps_tile" class="android.widget.TextView" package="com.android.settings" content-desc="Assistant, recent apps" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[72,700][1008,820]" />
    <node index="5" text="Airplane mode" resource-id="com.android.settings:id/airplane_switch" class="android.widget.Switch" package="com.android.settings" content-desc="" checkable="true" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[850,840][1008,940]" />
  </node>
</hierarchy>"#
}

/// Realistic Android Calculator application hierarchy XML fixture.
pub fn calculator_app_xml() -> &'static str {
    r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.google.android.calculator" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][1080,2400]">
    <node index="0" text="128" resource-id="com.google.android.calculator:id/result_final" class="android.widget.TextView" package="com.google.android.calculator" content-desc="Result: 128" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[60,200][1020,500]" />
    <node index="1" text="7" resource-id="com.google.android.calculator:id/digit_7" class="android.widget.Button" package="com.google.android.calculator" content-desc="7" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[60,1100][280,1280]" />
    <node index="2" text="8" resource-id="com.google.android.calculator:id/digit_8" class="android.widget.Button" package="com.google.android.calculator" content-desc="8" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[300,1100][520,1280]" />
    <node index="3" text="9" resource-id="com.google.android.calculator:id/digit_9" class="android.widget.Button" package="com.google.android.calculator" content-desc="9" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[540,1100][760,1280]" />
    <node index="4" text="+" resource-id="com.google.android.calculator:id/op_add" class="android.widget.Button" package="com.google.android.calculator" content-desc="plus" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[780,1100][1000,1280]" />
    <node index="5" text="4" resource-id="com.google.android.calculator:id/digit_4" class="android.widget.Button" package="com.google.android.calculator" content-desc="4" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[60,1300][280,1480]" />
    <node index="6" text="5" resource-id="com.google.android.calculator:id/digit_5" class="android.widget.Button" package="com.google.android.calculator" content-desc="5" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[300,1300][520,1480]" />
    <node index="7" text="6" resource-id="com.google.android.calculator:id/digit_6" class="android.widget.Button" package="com.google.android.calculator" content-desc="6" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[540,1300][760,1480]" />
    <node index="8" text="-" resource-id="com.google.android.calculator:id/op_sub" class="android.widget.Button" package="com.google.android.calculator" content-desc="minus" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[780,1300][1000,1480]" />
    <node index="9" text="=" resource-id="com.google.android.calculator:id/eq" class="android.widget.Button" package="com.google.android.calculator" content-desc="equals" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[780,1500][1000,1680]" />
  </node>
</hierarchy>"#
}

/// Realistic Android Lock Screen hierarchy XML fixture.
pub fn home_lock_screen_xml() -> &'static str {
    r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.android.systemui" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][1080,2400]">
    <node index="0" text="10:45" resource-id="com.android.systemui:id/clock_view" class="android.widget.TextView" package="com.android.systemui" content-desc="10:45 AM" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[200,300][880,500]" />
    <node index="1" text="Emergency" resource-id="com.android.systemui:id/emergency_call_button" class="android.widget.Button" package="com.android.systemui" content-desc="Emergency Call" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[80,2100][400,2240]" />
    <node index="2" text="Swipe up to unlock" resource-id="com.android.systemui:id/unlock_hint" class="android.widget.TextView" package="com.android.systemui" content-desc="Swipe up to unlock" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[340,2100][740,2240]" />
    <node index="3" text="" resource-id="com.android.systemui:id/camera_shortcut" class="android.widget.ImageView" package="com.android.systemui" content-desc="Camera" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[880,2100][1020,2240]" />
  </node>
</hierarchy>"#
}

/// Realistic Authentication / Form dialog hierarchy XML fixture.
pub fn login_form_dialog_xml() -> &'static str {
    r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.example.bank" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][1080,2400]">
    <node index="0" text="Sign In to Your Account" resource-id="com.example.bank:id/title" class="android.widget.TextView" package="com.example.bank" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[100,360][980,460]" />
    <node index="1" text="user@example.com" resource-id="com.example.bank:id/username" class="android.widget.EditText" package="com.example.bank" content-desc="Username or Email" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="true" scrollable="false" long-clickable="true" password="false" selected="false" bounds="[100,500][980,620]" />
    <node index="2" text="" resource-id="com.example.bank:id/password" class="android.widget.EditText" package="com.example.bank" content-desc="Password" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="true" password="true" selected="false" bounds="[100,660][980,780]" />
    <node index="3" text="Remember me" resource-id="com.example.bank:id/chk_remember" class="android.widget.CheckBox" package="com.example.bank" content-desc="Remember me option" checkable="true" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[100,820][500,900]" />
    <node index="4" text="Sign In" resource-id="com.example.bank:id/btn_login" class="android.widget.Button" package="com.example.bank" content-desc="Sign In" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[100,940][980,1060]" />
    <node index="5" text="Cancel" resource-id="com.example.bank:id/btn_cancel" class="android.widget.Button" package="com.example.bank" content-desc="Cancel and close dialog" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[100,1100][980,1220]" />
  </node>
</hierarchy>"#
}

package ai.ifay.fayger;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Color;
import android.os.Bundle;
import android.view.KeyEvent;
import android.view.Window;
import android.webkit.JavascriptInterface;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

/**
 * FayGer Mobile Pocket Guardian — Native Android Pairing & G0 Supervisory Shell.
 *
 * Zero-dependency pure Android SDK implementation (Activity + WebView) enabling
 * the Human Prime to pair with a local LAN FayGer Host (e.g. http://192.168.x.x:7979/mobile)
 * or operate the bundled offline preview console (FayID Vault, Faying Switch,
 * Red G0 Emergency Brake, CAP Remote Approval, and FHBP/1.0 Embodiment Telemetry).
 */
public final class MainActivity extends Activity {

    private static final String PREFS_NAME = "fayger_guardian_prefs";
    private static final String KEY_HOST_URL = "saved_host_url";
    private static final String FALLBACK_ASSET_URL = "file:///android_asset/index.html";

    private WebView webView;

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @SuppressWarnings("deprecation")
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        Window window = getWindow();
        if (window != null) {
            window.setStatusBarColor(Color.parseColor("#090D16"));
            window.setNavigationBarColor(Color.parseColor("#090D16"));
        }

        webView = new WebView(this);
        webView.setBackgroundColor(Color.parseColor("#090D16"));

        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);
        settings.setAllowFileAccess(true);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE);
        settings.setCacheMode(WebSettings.LOAD_DEFAULT);

        webView.setWebChromeClient(new WebChromeClient());
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                return false;
            }
        });

        webView.addJavascriptInterface(new NativePairingBridge(this), "FayGerNativeBridge");
        setContentView(webView);

        if (savedInstanceState != null) {
            webView.restoreState(savedInstanceState);
        } else {
            webView.loadUrl(FALLBACK_ASSET_URL);
        }
    }

    @Override
    protected void onSaveInstanceState(Bundle outState) {
        super.onSaveInstanceState(outState);
        if (webView != null) {
            webView.saveState(outState);
        }
    }

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        if (keyCode == KeyEvent.KEYCODE_BACK && webView != null && webView.canGoBack()) {
            webView.goBack();
            return true;
        }
        return super.onKeyDown(keyCode, event);
    }

    @Override
    protected void onDestroy() {
        if (webView != null) {
            webView.destroy();
            webView = null;
        }
        super.onDestroy();
    }

    public final class NativePairingBridge {
        private final SharedPreferences prefs;

        NativePairingBridge(Context context) {
            this.prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
        }

        @JavascriptInterface
        public String getAppVersion() {
            return "0.3.1";
        }

        @JavascriptInterface
        public String getSavedHostUrl() {
            return prefs.getString(KEY_HOST_URL, "");
        }

        @JavascriptInterface
        public void saveHostUrl(String url) {
            if (url == null) {
                return;
            }
            String trimmed = url.trim();
            if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
                prefs.edit().putString(KEY_HOST_URL, trimmed).apply();
            }
        }

        @JavascriptInterface
        public void openLauncherHome() {
            runOnUiThread(() -> {
                if (webView != null) {
                    webView.loadUrl(FALLBACK_ASSET_URL);
                }
            });
        }
    }
}

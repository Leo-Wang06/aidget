//! Host-side location for widgets that declare `permissions: ["location"]`.
//!
//! A widget never asks the webview for geolocation: the sandboxed iframe has an
//! opaque origin, and the engines refuse geolocation for it. Instead the page
//! calls `window.aidget.requestLocation()`, the shell forwards the request over
//! the Tauri IPC, and this module owns the one system location client.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::common::error::{Error, Result};
use crate::widgets::model::WidgetPermission;
use crate::widgets::store::Store;

/// A resolved location handed to a widget.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocationFix {
    pub latitude: f64,
    pub longitude: f64,
    /// Horizontal accuracy in metres.
    pub accuracy: f64,
    /// Reverse-geocoded neighbourhood or city; empty when unavailable.
    pub place: String,
}

/// Why a widget could not get a fix. Serialized as the JS `error.code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationError {
    /// The widget's config.json does not declare `location`.
    PermissionDenied,
    /// The platform refused access or the user declined the system prompt.
    Denied,
    /// The platform has no location provider, services are off, or no fix came.
    Unavailable,
    /// No callback before the deadline.
    Timeout,
}

impl LocationError {
    /// The string a widget sees as `error.code`.
    pub fn code(self) -> &'static str {
        match self {
            LocationError::PermissionDenied => "permission_denied",
            LocationError::Denied => "denied",
            LocationError::Unavailable => "unavailable",
            LocationError::Timeout => "timeout",
        }
    }
}

/// What the Tauri command rejects with, so the page can read `error.code`
/// instead of parsing a message.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationFailure {
    pub code: String,
    pub message: String,
}

impl From<&Error> for LocationFailure {
    fn from(error: &Error) -> Self {
        LocationFailure {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}

/// Check the widget's declaration before any native call happens. Kept
/// separate from the platform code so both paths share one gate.
pub fn authorize(store: &Store, id: &str) -> Result<()> {
    let meta = store.load_meta(id)?;
    if meta.grants(WidgetPermission::Location) {
        Ok(())
    } else {
        Err(Error::PermissionDenied(
            "widget does not declare permissions: [\"location\"]".to_string(),
        ))
    }
}

/// Ask for a single fix.
pub async fn request(app: &tauri::AppHandle, id: &str) -> Result<LocationFix> {
    let store = Store::new(
        app.state::<crate::runtime::ui::UiState>()
            .root
            .lock()
            .unwrap()
            .clone(),
    );
    authorize(&store, id)?;

    #[cfg(target_os = "macos")]
    return platform::request(app).await;
    // Windows drives its own async location object off-thread; nothing here
    // needs the app handle because consent is per request.
    #[cfg(windows)]
    {
        let _ = app;
        return platform::request().await;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    return platform::request(app).await;
    #[cfg(not(any(target_os = "macos", windows, all(unix, not(target_os = "macos")))))]
    {
        let _ = app;
        Err(Error::Location(LocationError::Unavailable))
    }
}

/// Windows location: consent once per request, then one `Geolocator` fix.
///
/// WinRT objects must live on an apartment-initialized thread and `join()`
/// blocks until the operation signals, so the work runs on its own thread and
/// answers over a oneshot channel.
#[cfg(windows)]
mod platform {
    use std::time::Duration;

    use windows::Devices::Geolocation::{
        GeolocationAccessStatus, Geolocator, Geoposition, PositionAccuracy,
    };
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    use super::{LocationError, LocationFix};
    use crate::common::error::{Error, Result};

    /// The platform's own wait, kept just inside the widget-facing timeout.
    const FIX_TIMEOUT: Duration = Duration::from_secs(18);

    fn unavailable<T>(result: windows::core::Result<T>) -> Result<T> {
        result.map_err(|_| Error::Location(LocationError::Unavailable))
    }

    pub async fn request() -> Result<LocationFix> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("aidget-location".to_string())
            .spawn(move || {
                let _ = tx.send(collect());
            })
            .map_err(|_| Error::Location(LocationError::Unavailable))?;
        // The thread bounds its own wait; this guard covers a wedged provider.
        match tokio::time::timeout(FIX_TIMEOUT + Duration::from_secs(2), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::Location(LocationError::Unavailable)),
            Err(_) => Err(Error::Location(LocationError::Timeout)),
        }
    }

    fn collect() -> Result<LocationFix> {
        // `RoInitialize` returns S_FALSE when the apartment is already set, and
        // `Result::from(HRESULT)` treats that as success.
        unavailable(unsafe { RoInitialize(RO_INIT_MULTITHREADED) })?;

        // Consent comes first: a locator created before an allowed prompt never
        // sees a position.
        let access = unavailable(Geolocator::RequestAccessAsync())?.join();
        match unavailable(access)? {
            GeolocationAccessStatus::Allowed => {}
            GeolocationAccessStatus::Denied | GeolocationAccessStatus::Unspecified => {
                return Err(Error::Location(LocationError::Denied));
            }
            _ => return Err(Error::Location(LocationError::Unavailable)),
        }

        let locator = unavailable(Geolocator::new())?;
        let _ = locator.SetDesiredAccuracy(PositionAccuracy::Default);
        let position = unavailable(locator.GetGeopositionAsync())?.join();
        let position = unavailable(position)?;
        let coordinate = unavailable(position.Coordinate())?;
        Ok(LocationFix {
            latitude: unavailable(coordinate.Latitude())?,
            longitude: unavailable(coordinate.Longitude())?,
            accuracy: unavailable(coordinate.Accuracy())?,
            place: place_name(&position),
        })
    }

    /// The civic address the platform resolved, if it resolved one at all.
    ///
    /// Windows errors instead of returning an empty address when no map data is
    /// available, so that is "no place" rather than a failed request.
    fn place_name(position: &Geoposition) -> String {
        let Ok(address) = position.CivicAddress() else {
            return String::new();
        };
        for value in [address.City(), address.State(), address.Country()] {
            let Ok(text) = value else { continue };
            let text = text.to_string();
            if !text.is_empty() {
                return text;
            }
        }
        String::new()
    }
}

/// Ask GeoClue through D-Bus, the desktop location service on Linux.
///
/// GeoClue only answers clients that name a installed desktop file, and its
/// permission prompt is the desktop's own, so a denial surfaces as a method
/// error and maps to `denied`.
#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use std::time::{Duration, Instant};

    use zbus::OwnedObjectPath;
    use zbus::Proxy;

    use super::{LocationError, LocationFix};
    use crate::common::error::{Error, Result};

    const SERVICE: &str = "org.freedesktop.GeoClue2";
    const MANAGER_PATH: &str = "/org/freedesktop/GeoClue2/Manager";
    const MANAGER_IFACE: &str = "org.freedesktop.GeoClue2.Manager";
    const CLIENT_IFACE: &str = "org.freedesktop.GeoClue2.Client";
    const LOCATION_IFACE: &str = "org.freedesktop.GeoClue2.Location";
    /// GeoClue waits for its own fix; aidget gives it slightly less time than
    /// the widget-facing deadline.
    const FIX_TIMEOUT: Duration = Duration::from_secs(18);

    pub async fn request(app: &tauri::AppHandle) -> Result<LocationFix> {
        let desktop_id = app.config().identifier.to_string();
        collect(desktop_id).await
    }

    /// A D-Bus-level failure means the service could not be reached at all.
    fn unavailable<T>(result: std::result::Result<T, zbus::Error>) -> Result<T> {
        result.map_err(|_| Error::Location(LocationError::Unavailable))
    }

    /// Call a method, mapping a method error reply — GeoClue refusing the
    /// request — to `denied`.
    fn denied<T>(result: std::result::Result<T, zbus::Error>) -> Result<T> {
        result.map_err(|error| match error {
            zbus::Error::MethodError(..) => Error::Location(LocationError::Denied),
            _ => Error::Location(LocationError::Unavailable),
        })
    }

    async fn collect(desktop_id: String) -> Result<LocationFix> {
        let connection = unavailable(zbus::Connection::session().await)?;
        let manager =
            unavailable(Proxy::new(&connection, SERVICE, MANAGER_PATH, MANAGER_IFACE).await)?;
        let client_path: OwnedObjectPath = denied(manager.call("GetClient", &()).await)?;
        let client =
            unavailable(Proxy::new(&connection, SERVICE, client_path, CLIENT_IFACE).await)?;
        client
            .set_property("DesktopId", desktop_id.as_str())
            .await
            .map_err(|_| Error::Location(LocationError::Unavailable))?;
        denied(client.call::<_, _, ()>("Start", &()).await)?;

        let location = match wait_for_location(&client).await {
            Ok(location) => location,
            Err(error) => {
                let _ = client.call_method("Stop", &()).await;
                return Err(error);
            }
        };
        let fix = read_fix(&location).await;
        let _ = client.call_method("Stop", &()).await;
        fix
    }

    /// Poll the client's `Location` property until GeoClue publishes an object.
    ///
    /// An unlocated client keeps reporting the root path, so only a real
    /// `/org/freedesktop/GeoClue2/Location/N` answers the wait.
    async fn wait_for_location(client: &Proxy<'_>) -> Result<Proxy<'static>> {
        let deadline = Instant::now() + FIX_TIMEOUT;
        loop {
            if let Ok(path) = client.get_property::<OwnedObjectPath>("Location").await {
                if path
                    .as_str()
                    .starts_with("/org/freedesktop/GeoClue2/Location/")
                {
                    let connection = client.connection().clone();
                    return unavailable(
                        Proxy::new_owned(connection, SERVICE, path, LOCATION_IFACE).await,
                    );
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::Location(LocationError::Timeout));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn read_fix(location: &Proxy<'_>) -> Result<LocationFix> {
        Ok(LocationFix {
            latitude: unavailable(location.get_property("Latitude").await)?,
            longitude: unavailable(location.get_property("Longitude").await)?,
            accuracy: unavailable(location.get_property("Accuracy").await)?,
            // GeoClue resolves coordinates only, so `place` stays empty rather
            // than being invented; aidget never falls back to IP geolocation.
            place: String::new(),
        })
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
mod platform {
    use std::cell::RefCell;
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
    use objc2_core_location::{
        CLAuthorizationStatus, CLGeocoder, CLLocation, CLLocationManager,
        CLLocationManagerDelegate, CLPlacemark,
    };
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSLocale, NSObject, NSObjectProtocol, NSString,
    };

    use super::{LocationError, LocationFix};
    use crate::common::error::{Error, Result};

    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

    /// One-shot slot shared by the delegate and the awaiting command.
    type Slot = Mutex<Option<tokio::sync::oneshot::Sender<Result<LocationFix>>>>;
    type SharedSlot = std::sync::Arc<Slot>;

    /// Serializes requests: one `CLLocationManager` serves the whole process,
    /// so two widgets must not race for its single pending callback.
    static ASKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    struct DelegateIvars {
        slot: RefCell<SharedSlot>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[ivars = DelegateIvars]
        struct LocationDelegate;

        unsafe impl NSObjectProtocol for LocationDelegate {}

        unsafe impl CLLocationManagerDelegate for LocationDelegate {
            #[unsafe(method(locationManager:didUpdateLocations:))]
            unsafe fn did_update_locations(
                &self,
                _manager: &CLLocationManager,
                locations: &NSArray<CLLocation>,
            ) {
                let Some(location) = locations.lastObject() else {
                    self.finish(Err(Error::Location(LocationError::Unavailable)));
                    return;
                };
                let coordinate = location.coordinate();
                let fix = LocationFix {
                    latitude: coordinate.latitude,
                    longitude: coordinate.longitude,
                    accuracy: location.horizontalAccuracy(),
                    place: String::new(),
                };
                // Geocoding is async too; resolve the slot once it lands.
                reverse_geocode(&location, self.slot(), fix);
            }

            #[unsafe(method(locationManager:didFailWithError:))]
            unsafe fn did_fail_with_error(
                &self,
                _manager: &CLLocationManager,
                error: &objc2_foundation::NSError,
            ) {
                let denied = error.code() == 1;
                let kind = if denied {
                    LocationError::Denied
                } else {
                    LocationError::Unavailable
                };
                self.finish(Err(Error::Location(kind)));
            }

            #[unsafe(method(locationManagerDidChangeAuthorization:))]
            unsafe fn did_change_authorization(&self, manager: &CLLocationManager) {
                match manager.authorizationStatus() {
                    CLAuthorizationStatus::NotDetermined => {}
                    CLAuthorizationStatus::Denied | CLAuthorizationStatus::Restricted => {
                        self.finish(Err(Error::Location(LocationError::Denied)));
                    }
                    _ => manager.requestLocation(),
                }
            }
        }
    );

    impl LocationDelegate {
        fn new(mtm: MainThreadMarker, slot: SharedSlot) -> Retained<Self> {
            let this = mtm.alloc::<Self>();
            let this = this.set_ivars(DelegateIvars {
                slot: RefCell::new(slot),
            });
            unsafe { msg_send![super(this), init] }
        }

        fn slot(&self) -> SharedSlot {
            self.ivars().slot.borrow().clone()
        }

        /// Point the delegate at the request currently being served.
        fn retarget(&self, slot: SharedSlot) {
            *self.ivars().slot.borrow_mut() = slot;
        }

        fn finish(&self, result: Result<LocationFix>) {
            if let Some(tx) = self.slot().lock().unwrap().take() {
                let _ = tx.send(result);
            }
        }
    }

    fn reverse_geocode(location: &CLLocation, slot: SharedSlot, fix: LocationFix) {
        let geocoder = unsafe { CLGeocoder::new() };
        let locale = NSLocale::localeWithLocaleIdentifier(&NSString::from_str("zh_CN"));
        let fallback = fix.clone();
        let slot_for_block = std::sync::Arc::clone(&slot);
        let block = RcBlock::new(
            move |placemarks: *mut NSArray<CLPlacemark>, error: *mut objc2_foundation::NSError| {
                let place = if error.is_null() && !placemarks.is_null() {
                    unsafe { place_name(&*placemarks) }
                } else {
                    String::new()
                };
                let resolved = LocationFix { place, ..fix };
                if let Some(tx) = slot_for_block.lock().unwrap().take() {
                    let _ = tx.send(Ok(resolved));
                }
            },
        );
        unsafe {
            geocoder.reverseGeocodeLocation_preferredLocale_completionHandler(
                location,
                Some(&locale),
                RcBlock::as_ptr(&block),
            );
        }
        // The geocoder keeps the handler until it calls back; leaking the
        // reference keeps the block alive without threading a lifetime here.
        std::mem::forget(block);

        // A geocode that never answers must not swallow a fix we already have.
        let slot_for_timeout = slot;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if let Some(tx) = slot_for_timeout.lock().unwrap().take() {
                let _ = tx.send(Ok(fallback));
            }
        });
    }

    /// macOS returns Traditional Chinese for `zh_CN` place names on some
    /// locales, so the first available admin unit is passed through unchanged
    /// and the card decides how to display it.
    unsafe fn place_name(placemarks: &NSArray<CLPlacemark>) -> String {
        let Some(mark) = placemarks.firstObject() else {
            return String::new();
        };
        for value in [mark.subLocality(), mark.locality()].into_iter().flatten() {
            let text = value.to_string();
            if !text.is_empty() {
                return text;
            }
        }
        mark.administrativeArea()
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    thread_local! {
        /// Owned on the main thread and never dropped: CoreLocation must be
        /// created and driven there, and callbacks arrive on the same loop.
        static MANAGER: RefCell<Option<(Retained<CLLocationManager>, Retained<LocationDelegate>)>> =
            const { RefCell::new(None) };
    }

    pub async fn request(app: &tauri::AppHandle) -> Result<LocationFix> {
        if unsafe { !CLLocationManager::locationServicesEnabled_class() } {
            return Err(Error::Location(LocationError::Unavailable));
        }

        let _guard = ASKING.lock().await;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let slot: SharedSlot = std::sync::Arc::new(Mutex::new(Some(tx)));
        let handle = app.clone();
        handle
            .run_on_main_thread(move || unsafe { begin(slot) })
            .map_err(|error| Error::Other(error.to_string()))?;

        match tokio::time::timeout(TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::Location(LocationError::Unavailable)),
            Err(_) => Err(Error::Location(LocationError::Timeout)),
        }
    }

    /// Main thread only. Reuses one manager and delegate, so a widget that
    /// refreshes every ten minutes does not leak CoreLocation objects.
    unsafe fn begin(slot: SharedSlot) {
        let Some(mtm) = MainThreadMarker::new() else {
            if let Some(tx) = slot.lock().unwrap().take() {
                let _ = tx.send(Err(Error::Location(LocationError::Unavailable)));
            }
            return;
        };

        MANAGER.with(|cell| {
            let mut state = cell.borrow_mut();
            if state.is_none() {
                let manager = CLLocationManager::new();
                manager.setDesiredAccuracy(100.0);
                let delegate = LocationDelegate::new(mtm, std::sync::Arc::clone(&slot));
                manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
                *state = Some((manager, delegate));
            }
            let Some((manager, delegate)) = state.as_mut() else {
                return;
            };

            delegate.retarget(std::sync::Arc::clone(&slot));

            match manager.authorizationStatus() {
                CLAuthorizationStatus::NotDetermined => manager.requestWhenInUseAuthorization(),
                CLAuthorizationStatus::Denied | CLAuthorizationStatus::Restricted => {
                    if let Some(tx) = slot.lock().unwrap().take() {
                        let _ = tx.send(Err(Error::Location(LocationError::Denied)));
                    }
                }
                _ => manager.requestLocation(),
            }
        });
    }
}

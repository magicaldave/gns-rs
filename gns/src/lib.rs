use crossbeam_queue::SegQueue;
use either::Either;
pub use gns_sys::bindings::*;
use std::{
    ffi::{c_void, CStr, CString},
    marker::PhantomData,
    mem::MaybeUninit,
    net::Ipv6Addr,
    pin::Pin,
    sync::atomic::AtomicBool,
};

pub type GnsMessageNumber = u64;

pub type GnsResult<T> = Result<T, EResult>;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct GnsError(EResult);

impl GnsError {
    pub fn into_result(self) -> GnsResult<()> {
        self.into()
    }
}

impl From<GnsError> for GnsResult<()> {
    fn from(GnsError(result): GnsError) -> Self {
        match result {
            EResult::k_EResultOK => Ok(()),
            e => Err(e),
        }
    }
}

static GNS_INIT: AtomicBool = AtomicBool::new(false);

pub struct GnsGlobal(());

impl Drop for GnsGlobal {
    fn drop(&mut self) {
        unsafe {
            GameNetworkingSockets_Kill();
        }
        GNS_INIT.store(false, std::sync::atomic::Ordering::SeqCst)
    }
}

impl GnsGlobal {
    pub fn get() -> Result<Self, String> {
        if GNS_INIT.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ) != Ok(false)
        {
            return Err("Only one handle of GnsGlobal must be held by a program. If the value is dropped, another handle might be created.".into());
        }
        unsafe {
            let mut error: SteamDatagramErrMsg = MaybeUninit::zeroed().assume_init();
            if !GameNetworkingSockets_Init(core::ptr::null(), &mut error) {
                GNS_INIT.store(false, std::sync::atomic::Ordering::SeqCst);
                Err(format!(
                    "{}",
                    core::str::from_utf8_unchecked(core::mem::transmute(&error[..]))
                ))
            } else {
                Ok(GnsGlobal(()))
            }
        }
    }
}

pub trait GnsDroppable: Sized {
    fn drop(&self, socket: &GnsSocket<Self>);
}

#[repr(transparent)]
pub struct GnsListenSocket(HSteamListenSocket);

#[repr(transparent)]
pub struct GnsPollGroup(HSteamNetPollGroup);

pub struct IsCreated;

impl GnsDroppable for IsCreated {
    fn drop(&self, _: &GnsSocket<Self>) {}
}

pub trait IsReady: GnsDroppable {
    fn queue(&self) -> &SegQueue<SteamNetConnectionStatusChangedCallback_t>;
    fn receive<const K: usize>(
        &self,
        gns: &GnsSocket<Self>,
        messages: &mut [GnsNetworkMessage<ToReceive>; K],
    ) -> usize;
}

pub struct IsServer {
    queue: Pin<Box<SegQueue<SteamNetConnectionStatusChangedCallback_t>>>,
    listen_socket: GnsListenSocket,
    poll_group: GnsPollGroup,
}

impl GnsDroppable for IsServer {
    fn drop(&self, gns: &GnsSocket<Self>) {
        unsafe {
            SteamAPI_ISteamNetworkingSockets_CloseListenSocket(gns.socket, self.listen_socket.0);
            SteamAPI_ISteamNetworkingSockets_DestroyPollGroup(gns.socket, self.poll_group.0);
        }
    }
}

impl IsReady for IsServer {
    #[inline]
    fn queue(&self) -> &SegQueue<SteamNetConnectionStatusChangedCallback_t> {
        &self.queue
    }

    #[inline]
    fn receive<const K: usize>(
        &self,
        gns: &GnsSocket<Self>,
        messages: &mut [GnsNetworkMessage<ToReceive>; K],
    ) -> usize {
        unsafe {
            SteamAPI_ISteamNetworkingSockets_ReceiveMessagesOnPollGroup(
                gns.socket,
                self.poll_group.0,
                messages.as_mut_ptr() as _,
                K as _,
            ) as _
        }
    }
}

pub struct IsClient {
    queue: Pin<Box<SegQueue<SteamNetConnectionStatusChangedCallback_t>>>,
    connection: GnsConnection,
}

impl GnsDroppable for IsClient {
    fn drop(&self, gns: &GnsSocket<Self>) {
        unsafe {
            SteamAPI_ISteamNetworkingSockets_CloseConnection(
                gns.socket,
                self.connection.0,
                0,
                core::ptr::null(),
                false,
            );
        }
    }
}

impl IsReady for IsClient {
    #[inline]
    fn queue(&self) -> &SegQueue<SteamNetConnectionStatusChangedCallback_t> {
        &self.queue
    }

    #[inline]
    fn receive<const K: usize>(
        &self,
        gns: &GnsSocket<Self>,
        messages: &mut [GnsNetworkMessage<ToReceive>; K],
    ) -> usize {
        unsafe {
            SteamAPI_ISteamNetworkingSockets_ReceiveMessagesOnConnection(
                gns.socket,
                self.connection.0,
                messages.as_mut_ptr() as _,
                K as _,
            ) as _
        }
    }
}

pub trait MayDrop {
    const MUST_DROP: bool;
}

pub struct ToSend(());

impl MayDrop for ToSend {
    const MUST_DROP: bool = false;
}

pub struct ToReceive(());

impl MayDrop for ToReceive {
    const MUST_DROP: bool = true;
}

pub type Priority = u32;
pub type Weight = u16;
pub type GnsLane = (Priority, Weight);
pub type GnsLaneId = u16;

#[repr(transparent)]
pub struct GnsNetworkMessage<T: MayDrop>(*mut ISteamNetworkingMessage, PhantomData<T>);

impl<T> Drop for GnsNetworkMessage<T>
where
    T: MayDrop,
{
    fn drop(&mut self) {
        if T::MUST_DROP && !self.0.is_null() {
            unsafe {
                SteamAPI_SteamNetworkingMessage_t_Release(self.0);
            }
        }
    }
}

impl<T> GnsNetworkMessage<T>
where
    T: MayDrop,
{
    #[inline]
    pub unsafe fn into_inner(self) -> *mut ISteamNetworkingMessage {
        self.0
    }

    #[inline]
    pub fn payload(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts((*self.0).m_pData as *const u8, (*self.0).m_cbSize as _)
        }
    }

    #[inline]
    pub fn message_number(&self) -> u64 {
        unsafe { (*self.0).m_nMessageNumber as _ }
    }

    #[inline]
    pub fn lane(&self) -> GnsLaneId {
        unsafe { (*self.0).m_idxLane }
    }

    #[inline]
    pub fn flags(&self) -> u32 {
        unsafe { (*self.0).m_nFlags as _ }
    }

    #[inline]
    pub fn user_data(&self) -> u64 {
        unsafe { (*self.0).m_nUserData as _ }
    }

    #[inline]
    pub fn connection(&self) -> GnsConnection {
        GnsConnection(unsafe { (*self.0).m_conn })
    }

    pub fn connection_user_data(&self) -> u64 {
        unsafe { (*self.0).m_nConnUserData as _ }
    }
}

impl GnsNetworkMessage<ToSend> {
    #[inline]
    fn new(
        ptr: *mut ISteamNetworkingMessage,
        conn: GnsConnection,
        flags: i32,
        payload: &[u8],
    ) -> Self {
        GnsNetworkMessage(ptr, PhantomData)
            .set_flags(flags)
            .set_payload(payload)
            .set_connection(conn)
    }

    #[inline]
    pub fn set_connection(self, GnsConnection(conn): GnsConnection) -> Self {
        unsafe { (*self.0).m_conn = conn }
        self
    }

    #[inline]
    pub fn set_payload(self, payload: &[u8]) -> Self {
        unsafe {
            core::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                (*self.0).m_pData as *mut u8,
                payload.len(),
            );
        }
        self
    }

    #[inline]
    pub fn set_lane(self, lane: u16) -> Self {
        unsafe { (*self.0).m_idxLane = lane }
        self
    }

    #[inline]
    pub fn set_flags(self, flags: i32) -> Self {
        unsafe { (*self.0).m_nFlags = flags as _ }
        self
    }

    #[inline]
    pub fn set_user_data(self, userdata: u64) -> Self {
        unsafe { (*self.0).m_nUserData = userdata as _ }
        self
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GnsConnection(HSteamNetConnection);

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct GnsConnectionInfo(SteamNetConnectionInfo_t);

impl GnsConnectionInfo {
    #[inline]
    pub fn state(&self) -> ESteamNetworkingConnectionState {
        self.0.m_eState
    }

    /// See ESteamNetConnectionEnd
    #[inline]
    pub fn end_reason(&self) -> u32 {
        self.0.m_eEndReason as u32
    }

    #[inline]
    pub fn remote_address(&self) -> Ipv6Addr {
        Ipv6Addr::from(unsafe { self.0.m_addrRemote.__bindgen_anon_1.m_ipv6 })
    }

    #[inline]
    pub fn remote_port(&self) -> u16 {
        self.0.m_addrRemote.m_port
    }
}

#[repr(transparent)]
#[derive(Debug, Default, Copy, Clone, Hash, PartialOrd, Ord, PartialEq, Eq)]
pub struct GnsConnectionRealTimeLaneStatus(SteamNetConnectionRealTimeLaneStatus_t);

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct GnsConnectionRealTimeStatus(SteamNetConnectionRealTimeStatus_t);

impl GnsConnectionRealTimeStatus {
    #[inline]
    pub fn state(&self) -> ESteamNetworkingConnectionState {
        self.0.m_eState
    }

    #[inline]
    pub fn ping(&self) -> u32 {
        self.0.m_nPing as _
    }

    #[inline]
    pub fn quality_local(&self) -> f32 {
        self.0.m_flConnectionQualityLocal
    }

    #[inline]
    pub fn quality_remote(&self) -> f32 {
        self.0.m_flConnectionQualityRemote
    }

    #[inline]
    pub fn out_packets_per_sec(&self) -> f32 {
        self.0.m_flOutPacketsPerSec
    }

    #[inline]
    pub fn out_bytes_per_sec(&self) -> f32 {
        self.0.m_flOutBytesPerSec
    }

    #[inline]
    pub fn in_packets_per_sec(&self) -> f32 {
        self.0.m_flInPacketsPerSec
    }

    #[inline]
    pub fn in_bytes_per_sec(&self) -> f32 {
        self.0.m_flInBytesPerSec
    }

    #[inline]
    pub fn send_rate_bytes_per_sec(&self) -> u32 {
        self.0.m_nSendRateBytesPerSecond as _
    }

    #[inline]
    pub fn pending_bytes_unreliable(&self) -> u32 {
        self.0.m_cbPendingUnreliable as _
    }

    #[inline]
    pub fn pending_bytes_reliable(&self) -> u32 {
        self.0.m_cbPendingReliable as _
    }

    #[inline]
    pub fn bytes_sent_unacked_reliable(&self) -> u32 {
        self.0.m_cbSentUnackedReliable as _
    }

    #[inline]
    pub fn approximated_queue_time(&self) -> u64 {
        self.0.m_usecQueueTime as _
    }
}

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct GnsConnectionEvent(SteamNetConnectionStatusChangedCallback_t);

impl GnsConnectionEvent {
    #[inline]
    pub fn old_state(&self) -> ESteamNetworkingConnectionState {
        self.0.m_eOldState
    }

    #[inline]
    pub fn connection(&self) -> GnsConnection {
        GnsConnection(self.0.m_hConn)
    }

    #[inline]
    pub fn info(&self) -> GnsConnectionInfo {
        GnsConnectionInfo(self.0.m_info)
    }
}

pub struct GnsSocket<'x, S: GnsDroppable> {
    global: &'x GnsGlobal,
    utils: GnsUtils,
    socket: *mut ISteamNetworkingSockets,
    state: S,
}

impl<'x, S> Drop for GnsSocket<'x, S>
where
    S: GnsDroppable,
{
    fn drop(&mut self) {
        self.state.drop(&self);
    }
}

impl<'x, S> GnsSocket<'x, S>
where
    S: GnsDroppable,
{
    #[inline]
    pub unsafe fn into_inner(self) -> *mut ISteamNetworkingSockets {
        self.socket
    }

    #[inline]
    pub fn utils(&self) -> &GnsUtils {
        &self.utils
    }
}

impl<'x, S> GnsSocket<'x, S>
where
    S: GnsDroppable + IsReady,
{
    #[inline]
    pub fn get_connection_real_time_status(
        &self,
        GnsConnection(conn): GnsConnection,
        nb_of_lanes: u32,
    ) -> GnsResult<(
        GnsConnectionRealTimeStatus,
        Vec<GnsConnectionRealTimeLaneStatus>,
    )> {
        let mut lanes: Vec<GnsConnectionRealTimeLaneStatus> =
            vec![Default::default(); nb_of_lanes as _];
        unsafe {
            let mut status: GnsConnectionRealTimeStatus = MaybeUninit::zeroed().assume_init();
            GnsError(
                SteamAPI_ISteamNetworkingSockets_GetConnectionRealTimeStatus(
                    self.socket,
                    conn,
                    &mut status as *mut GnsConnectionRealTimeStatus
                        as *mut SteamNetConnectionRealTimeStatus_t,
                    nb_of_lanes as _,
                    lanes.as_mut_ptr() as *mut GnsConnectionRealTimeLaneStatus
                        as *mut SteamNetConnectionRealTimeLaneStatus_t,
                ),
            )
            .into_result()?;
            Ok((status, lanes))
        }
    }

    #[inline]
    pub fn get_connection_info(
        &self,
        GnsConnection(conn): GnsConnection,
    ) -> Option<GnsConnectionInfo> {
        unsafe {
            let mut info: SteamNetConnectionInfo_t = MaybeUninit::zeroed().assume_init();
            if SteamAPI_ISteamNetworkingSockets_GetConnectionInfo(self.socket, conn, &mut info) {
                Some(GnsConnectionInfo(info))
            } else {
                None
            }
        }
    }

    #[inline]
    pub fn flush_messages_on_connection(
        &self,
        GnsConnection(conn): GnsConnection,
    ) -> GnsResult<()> {
        GnsError(unsafe {
            SteamAPI_ISteamNetworkingSockets_FlushMessagesOnConnection(self.socket, conn)
        })
        .into_result()
    }

    #[inline]
    pub fn close_connection(
        &self,
        GnsConnection(conn): GnsConnection,
        reason: u32,
        debug: &str,
        linger: bool,
    ) -> bool {
        let debug_c = CString::new(debug).expect("str; qed;");
        unsafe {
            SteamAPI_ISteamNetworkingSockets_CloseConnection(
                self.socket,
                conn,
                reason as _,
                debug_c.as_ptr(),
                linger,
            )
        }
    }

    #[inline]
    pub fn poll_messages<const K: usize, F>(&self, mut message_callback: F) -> usize
    where
        F: FnMut(&GnsNetworkMessage<ToReceive>),
    {
        let mut messages: [GnsNetworkMessage<ToReceive>; K] =
            unsafe { MaybeUninit::zeroed().assume_init() };
        let nb_of_messages = self.state.receive(&self, &mut messages);
        for message in messages.into_iter().take(nb_of_messages) {
            message_callback(&message);
        }
        nb_of_messages
    }

    #[inline]
    pub fn poll_event<const K: usize, F>(&self, mut event_callback: F) -> usize
    where
        F: FnMut(GnsConnectionEvent),
    {
        let mut processed = 0;
        'a: while let Some(event) = self.state.queue().pop() {
            event_callback(GnsConnectionEvent(event));
            processed += 1;
            if processed == K {
                break 'a;
            }
        }
        processed
    }

    #[inline]
    pub fn poll_callbacks(&self) {
        unsafe {
            SteamAPI_ISteamNetworkingSockets_RunCallbacks(self.socket);
        }
    }

    #[inline]
    pub fn configure_connection_lanes(
        &self,
        GnsConnection(connection): GnsConnection,
        lanes: &[GnsLane],
    ) -> GnsResult<()> {
        let (priorities, weights): (Vec<_>, Vec<_>) = lanes.iter().copied().unzip();
        GnsError(unsafe {
            SteamAPI_ISteamNetworkingSockets_ConfigureConnectionLanes(
                self.socket,
                connection,
                lanes.len() as _,
                priorities.as_ptr() as *const u32 as *const i32,
                weights.as_ptr(),
            )
        })
        .into_result()
    }

    #[inline]
    pub fn send_messages(
        &self,
        messages: &[GnsNetworkMessage<ToSend>],
    ) -> Vec<Either<GnsMessageNumber, EResult>> {
        let mut result = vec![0i64; messages.len()];
        unsafe {
            SteamAPI_ISteamNetworkingSockets_SendMessages(
                self.socket,
                messages.len() as _,
                messages.as_ptr() as *const _,
                result.as_mut_ptr(),
            );
        }
        result
            .into_iter()
            .map(|value| {
                if value < 0 {
                    Either::Right(unsafe { core::mem::transmute((-value) as u32) })
                } else {
                    Either::Left(value as _)
                }
            })
            .collect()
    }
}

impl<'x> GnsSocket<'x, IsCreated> {
    unsafe extern "C" fn on_connection_state_changed(
        info: &SteamNetConnectionStatusChangedCallback_t,
    ) {
        let queue = &*(info.m_info.m_nUserData as *const u64
            as *const SegQueue<SteamNetConnectionStatusChangedCallback_t>);
        queue.push(*info);
    }

    #[inline]
    pub fn new(global: &'x GnsGlobal) -> Option<Self> {
        let utils = GnsUtils::new()?;
        let ptr = unsafe { SteamAPI_SteamNetworkingSockets_v009() };
        if ptr.is_null() {
            None
        } else {
            Some(GnsSocket {
                global,
                utils,
                socket: ptr,
                state: IsCreated,
            })
        }
    }

    #[inline]
    fn setup_common(
        address: Ipv6Addr,
        port: u16,
        queue: &SegQueue<SteamNetConnectionStatusChangedCallback_t>,
    ) -> (SteamNetworkingIPAddr, [SteamNetworkingConfigValue_t; 2]) {
        let addr = SteamNetworkingIPAddr {
            __bindgen_anon_1: SteamNetworkingIPAddr__bindgen_ty_2 {
                m_ipv6: address.octets(),
            },
            m_port: port,
        };
        let options = [SteamNetworkingConfigValue_t {
            m_eDataType: ESteamNetworkingConfigDataType::k_ESteamNetworkingConfig_Ptr,
            m_eValue: ESteamNetworkingConfigValue::k_ESteamNetworkingConfig_Callback_ConnectionStatusChanged,
            m_val: SteamNetworkingConfigValue_t__bindgen_ty_1 {
              m_ptr: Self::on_connection_state_changed as *const fn(&SteamNetConnectionStatusChangedCallback_t) as *mut c_void
            }
          }, SteamNetworkingConfigValue_t {
            m_eDataType: ESteamNetworkingConfigDataType::k_ESteamNetworkingConfig_Int64,
            m_eValue: ESteamNetworkingConfigValue::k_ESteamNetworkingConfig_ConnectionUserData,
            m_val: SteamNetworkingConfigValue_t__bindgen_ty_1 {
              m_int64: queue as *const _ as i64
            }
        }];

        (addr, options)
    }

    #[inline]
    pub fn listen(self, address: Ipv6Addr, port: u16) -> Result<GnsSocket<'x, IsServer>, ()> {
        let queue = Box::pin(SegQueue::new());
        let (addr, options) = Self::setup_common(address, port, &queue);
        let listen_socket = unsafe {
            SteamAPI_ISteamNetworkingSockets_CreateListenSocketIP(
                self.socket,
                &addr,
                options.len() as _,
                options.as_ptr(),
            )
        };
        if listen_socket == k_HSteamListenSocket_Invalid {
            Err(())
        } else {
            let poll_group =
                unsafe { SteamAPI_ISteamNetworkingSockets_CreatePollGroup(self.socket) };
            if poll_group == k_HSteamNetPollGroup_Invalid {
                Err(())
            } else {
                Ok(GnsSocket {
                    global: self.global,
                    utils: self.utils,
                    socket: self.socket,
                    state: IsServer {
                        queue,
                        listen_socket: GnsListenSocket(listen_socket),
                        poll_group: GnsPollGroup(poll_group),
                    },
                })
            }
        }
    }

    #[inline]
    pub fn connect(self, address: Ipv6Addr, port: u16) -> Result<GnsSocket<'x, IsClient>, ()> {
        let queue = Box::pin(SegQueue::new());
        let (addr, options) = Self::setup_common(address, port, &queue);
        let connection = unsafe {
            SteamAPI_ISteamNetworkingSockets_ConnectByIPAddress(
                self.socket,
                &addr,
                options.len() as _,
                options.as_ptr(),
            )
        };
        if connection == k_HSteamNetConnection_Invalid {
            Err(())
        } else {
            Ok(GnsSocket {
                global: self.global,
                utils: self.utils,
                socket: self.socket,
                state: IsClient {
                    queue,
                    connection: GnsConnection(connection),
                },
            })
        }
    }
}

impl<'x> GnsSocket<'x, IsServer> {
    #[inline]
    pub fn accept(&self, connection: GnsConnection) -> GnsResult<()> {
        GnsError(unsafe {
            SteamAPI_ISteamNetworkingSockets_AcceptConnection(self.socket, connection.0)
        })
        .into_result()?;
        if !unsafe {
            SteamAPI_ISteamNetworkingSockets_SetConnectionPollGroup(
                self.socket,
                connection.0,
                self.state.poll_group.0,
            )
        } {
            Err(EResult::k_EResultInvalidState)
        } else {
            Ok(())
        }
    }
}

impl<'x> GnsSocket<'x, IsClient> {
    #[inline]
    pub fn connection(&self) -> GnsConnection {
        self.state.connection
    }
}

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct GnsUtils(*mut ISteamNetworkingUtils);

impl GnsUtils {
    #[inline]
    fn new() -> Option<Self> {
        let ptr = unsafe { SteamAPI_SteamNetworkingUtils_v003() };
        if ptr.is_null() {
            None
        } else {
            Some(GnsUtils(ptr))
        }
    }

    #[inline]
    pub fn enable_debug_output(&self, ty: ESteamNetworkingSocketsDebugOutputType) {
        unsafe extern "C" fn debug(ty: ESteamNetworkingSocketsDebugOutputType, msg: *const i8) {
            println!("{:#?}: {}", ty, CStr::from_ptr(msg).to_string_lossy());
        }
        unsafe {
            SteamAPI_ISteamNetworkingUtils_SetDebugOutputFunction(self.0, ty, Some(debug));
        }
    }

    #[inline]
    pub fn allocate_message(
        &self,
        conn: GnsConnection,
        flags: i32,
        payload: &[u8],
    ) -> GnsNetworkMessage<ToSend> {
        let message_ptr =
            unsafe { SteamAPI_ISteamNetworkingUtils_AllocateMessage(self.0, payload.len() as _) };
        GnsNetworkMessage::new(message_ptr, conn, flags, payload)
    }

    #[inline]
    pub unsafe fn into_inner(self) -> *mut ISteamNetworkingUtils {
        self.0
    }
}

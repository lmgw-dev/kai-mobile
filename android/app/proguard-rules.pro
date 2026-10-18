# R8 full mode (design §2.2). UniFFI's Kotlin bindings reach the native library through JNA, which
# finds fields and methods by reflection: keep JNA and the generated bindings whole.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class dev.rootshell.kai.core.ffi.** { *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.platform.**
# libwebrtc's Java side is reached from native code by name (JNI): keep org.webrtc whole.
-keep class org.webrtc.** { *; }
-dontwarn org.webrtc.**
